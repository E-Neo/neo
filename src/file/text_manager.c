/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "file/text_manager.h"

#include "core/result.h"
#include "core/slice.h"
#include "core/string.h"
#include "core/vec.h"
#include <stddef.h>

TextManager
TextManager_new (String path, String content)
{
  Vec_size_t lines = Vec_size_t_new ();
  const char *content_cbegin = String_cbegin (&content);
  size_t content_len = String_len (&content);
  size_t index = 0;
  do
    {
      Vec_size_t_push (&lines, index);
      while (index < content_len && content_cbegin[index] != '\n')
        {
          index++;
        }
      index++;
    }
  while (index < content_len);
  return (TextManager){ .path_ = path, .content_ = content, .lines_ = lines };
}

void
TextManager_drop (TextManager *self)
{
  String_drop (&self->path_);
  String_drop (&self->content_);
  Vec_size_t_drop (&self->lines_);
}

const String *
TextManager_get_path (const TextManager *self)
{
  return &self->path_;
}

const String *
TextManager_get_content (const TextManager *self)
{
  return &self->content_;
}

static int
cmp_size_t (const size_t *key, const size_t *item)
{
  if (*key == *item)
    {
      return 0;
    }
  else if (*key < *item)
    {
      return -1;
    }
  else
    {
      return 1;
    }
}

static size_t
TextManager_lookup_line (const TextManager *self, size_t offset)
{
  size_t content_len = String_len (TextManager_get_content (self));
  Slice_const_size_t lines = Slice_const_size_t_new (
      Vec_size_t_cbegin (&self->lines_), Vec_size_t_len (&self->lines_));
  if (offset > content_len)
    {
      return 0;
    }
  if (offset == content_len)
    {
      return Slice_const_size_t_len (&lines);
    }
  Result_size_t_size_t res
      = Slice_const_size_t_binary_search (&lines, &offset, cmp_size_t);
  return Result_size_t_size_t_is_ok (&res)
             ? Result_size_t_size_t_unwrap (&res) + 1
             : Result_size_t_size_t_unwrap_err (&res);
}

Position
TextManager_lookup_position (const TextManager *self, size_t offset)
{
  size_t line = TextManager_lookup_line (self, offset);
  return Position_new (
      line,
      line == 0 ? 0 : offset - Vec_size_t_cbegin (&self->lines_)[line - 1]);
}
