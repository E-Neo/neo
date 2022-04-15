/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "span.h"

#include <assert.h>
#include <stddef.h>
#include <string.h>

#include "result.h"
#include "string.h"
#include "vec.h"

Span
Span_new (const char *begin, size_t len)
{
  return (Span){ .begin_ = begin, .end_ = begin + len };
}

Span
Span_from_string (const String *str)
{
  return Span_new (String_cbegin (str), String_len (str));
}

Span
Span_from_cstring (const char *str)
{
  return Span_new (str, strlen (str));
}

const char *
Span_cbegin (const Span *self)
{
  return self->begin_;
}

const char *
Span_cend (const Span *self)
{
  return self->end_;
}

size_t
Span_len (const Span *self)
{
  return Span_cend (self) - Span_cbegin (self);
}

int
Span_cmp (const Span *self, const Span *other)
{
  const char *self_ptr = Span_cbegin (self);
  const char *other_ptr = Span_cbegin (other);
  while (self_ptr < Span_cend (self) && other_ptr < Span_cend (other))
    {
      if (*self_ptr < *other_ptr)
        {
          return -1;
        }
      else if (*self_ptr > *other_ptr)
        {
          return 1;
        }
      self_ptr++;
      other_ptr++;
    }
  if (self_ptr < Span_cend (self))
    {
      return 1;
    }
  else if (other_ptr < Span_cend (other))
    {
      return -1;
    }
  else
    {
      return 0;
    }
}

int
Span_cmp_cstring (const Span *self, const char *cstr)
{
  const char *span_begin = Span_cbegin (self);
  size_t span_len = Span_len (self);
  for (size_t i = 0; i < span_len; i++)
    {
      if (cstr[i] == '\0')
        {
          return 1;
        }
      if (span_begin[i] < cstr[i])
        {
          return -1;
        }
      else if (span_begin[i] > cstr[i])
        {
          return 1;
        }
    }
  return cstr[span_len] == '\0' ? 0 : -1;
}

Position
Position_new (size_t line, size_t column)
{
  return (Position){ .line_ = line, .column_ = column };
}

size_t
Position_get_line (const Position *self)
{
  return self->line_;
}

size_t
Position_get_column (const Position *self)
{
  return self->column_;
}

SourceFile
SourceFile_new (String path, String content)
{
  Vec_const_char_ptr lines = Vec_const_char_ptr_new ();
  const char *ptr = String_cbegin (&content);
  do
    {
      Vec_const_char_ptr_push (&lines, ptr);
      while (ptr < String_cend (&content) && *ptr != '\n')
        {
          ptr++;
        }
      ptr++;
    }
  while (ptr <= String_cend (&content));
  return (SourceFile){ .path_ = path, .content_ = content, .lines_ = lines };
}

void
SourceFile_drop (SourceFile *self)
{
  String_drop (&self->path_);
  String_drop (&self->content_);
  Vec_const_char_ptr_drop (&self->lines_);
}

const String *
SourceFile_get_path (const SourceFile *self)
{
  return &self->path_;
}

const String *
SourceFile_get_content (const SourceFile *self)
{
  return &self->content_;
}

static int
compare_const_char_ptr (const char *key, const char *item)
{
  return key - item;
}

static size_t
SourceFile_lookup_line (const SourceFile *self, const char *pos)
{
  if (pos < String_cbegin (SourceFile_get_content (self))
      || pos > String_cend (SourceFile_get_content (self)))
    {
      return 0;
    }
  if (pos == String_cend (SourceFile_get_content (self)))
    {
      return Vec_const_char_ptr_len (&self->lines_);
    }
  Result_size_t_size_t res = Vec_const_char_ptr_binary_search_by (
      &self->lines_, &pos, compare_const_char_ptr);
  if (Result_size_t_size_t_is_ok (&res))
    {
      return Result_size_t_size_t_unwrap (&res) + 1;
    }
  return Result_size_t_size_t_unwrap_err (&res);
}

Position
SourceFile_lookup_position (const SourceFile *self, const char *pos)
{
  size_t line = SourceFile_lookup_line (self, pos);
  if (line)
    {
      return Position_new (
          line, pos - Vec_const_char_ptr_cbegin (&self->lines_)[line - 1]);
    }
  return Position_new (0, 0);
}

Span
SourceFile_get_line (const SourceFile *self, size_t line)
{
  assert (line);
  const char *begin = Vec_const_char_ptr_cbegin (&self->lines_)[line - 1];
  const char *end = line >= Vec_const_char_ptr_len (&self->lines_)
                        ? String_cend (&self->content_)
                        : Vec_const_char_ptr_cbegin (&self->lines_)[line];
  return Span_new (begin, end - begin);
}

#ifdef TESTS
#include "test.h"

NEO_TEST (test_span_cmp_cstring_00)
{
  char content[] = "true";
  Span span = Span_new (content, sizeof (content) - 1);
  ASSERT_U64_EQ (Span_len (&span), 4);
  ASSERT_I64_EQ (Span_cmp_cstring (&span, "true"), 0);
  ASSERT_I64_EQ (Span_cmp_cstring (&span, "tru"), 1);
  ASSERT_I64_EQ (Span_cmp_cstring (&span, "truefalse"), -1);
  ASSERT_I64_EQ (Span_cmp_cstring (&span, "srue"), 1);
  ASSERT_I64_EQ (Span_cmp_cstring (&span, "tque"), 1);
  ASSERT_I64_EQ (Span_cmp_cstring (&span, "urue"), -1);
  ASSERT_I64_EQ (Span_cmp_cstring (&span, "tsue"), -1);
}

static SourceFile
SourceFile_new_test (const char *content)
{
  return SourceFile_new (String_from_cstring ("test"),
                         String_from_cstring (content));
}

NEO_TEST (test_source_file_lookup_line_00)
{
  SourceFile file = SourceFile_new_test ("if true {\n"
                                         "  true\n"
                                         "} else {\n"
                                         "  false\n"
                                         "}");
  ASSERT_U64_EQ (Vec_const_char_ptr_len (&file.lines_), 5);
  const String *content = SourceFile_get_content (&file);
  size_t offsets[] = { 0, 10, 17, 26, 34 };
  for (size_t i = 0; i < Vec_const_char_ptr_len (&file.lines_); i++)
    {
      ASSERT_U64_EQ (Vec_const_char_ptr_cbegin (&file.lines_)[i]
                         - (String_cbegin (content) + offsets[i]),
                     0);
    }
  ASSERT_U64_EQ (SourceFile_lookup_line (&file, NULL), 0);
  ASSERT_U64_EQ (SourceFile_lookup_line (&file, String_cend (content) + 1), 0);
  ASSERT_U64_EQ (SourceFile_lookup_line (&file, String_cbegin (content)), 1);
  ASSERT_U64_EQ (SourceFile_lookup_line (&file, String_cbegin (content) + 12),
                 2);
  ASSERT_U64_EQ (SourceFile_lookup_line (&file, String_cend (content)), 5);
  SourceFile_drop (&file);
}

NEO_TEST (test_source_file_lookup_line_01)
{
  SourceFile file = SourceFile_new_test ("if true {\n"
                                         "  true\n"
                                         "} else {\n"
                                         "  false\n"
                                         "}\n");
  ASSERT_U64_EQ (Vec_const_char_ptr_len (&file.lines_), 6);
  const String *content = SourceFile_get_content (&file);
  size_t offsets[] = { 0, 10, 17, 26, 34, 36 };
  for (size_t i = 0; i < Vec_const_char_ptr_len (&file.lines_); i++)
    {
      ASSERT_U64_EQ (Vec_const_char_ptr_cbegin (&file.lines_)[i]
                         - (String_cbegin (content) + offsets[i]),
                     0);
    }
  ASSERT_U64_EQ (SourceFile_lookup_line (&file, NULL), 0);
  ASSERT_U64_EQ (SourceFile_lookup_line (&file, String_cend (content) + 1), 0);
  ASSERT_U64_EQ (SourceFile_lookup_line (&file, String_cbegin (content)), 1);
  ASSERT_U64_EQ (SourceFile_lookup_line (&file, String_cbegin (content) + 10),
                 2);
  ASSERT_U64_EQ (SourceFile_lookup_line (&file, String_cend (content)), 6);
  SourceFile_drop (&file);
}

NEO_TEST (test_source_file_lookup_position_00)
{
  SourceFile file = SourceFile_new_test ("true");
  const String *content = SourceFile_get_content (&file);
  Position pos;
  pos = SourceFile_lookup_position (&file, String_cbegin (content));
  ASSERT_U64_EQ (pos.line_, 1);
  ASSERT_U64_EQ (pos.column_, 0);
  pos = SourceFile_lookup_position (&file, String_cend (content));
  ASSERT_U64_EQ (pos.line_, 1);
  ASSERT_U64_EQ (pos.column_, 4);
  SourceFile_drop (&file);
}

NEO_TEST (test_source_file_lookup_position_01)
{
  SourceFile file = SourceFile_new_test ("true\n");
  const String *content = SourceFile_get_content (&file);
  Position pos;
  pos = SourceFile_lookup_position (&file, String_cbegin (content));
  ASSERT_U64_EQ (pos.line_, 1);
  ASSERT_U64_EQ (pos.column_, 0);
  pos = SourceFile_lookup_position (&file, String_cbegin (content) + 3);
  ASSERT_U64_EQ (pos.line_, 1);
  ASSERT_U64_EQ (pos.column_, 3);
  SourceFile_drop (&file);
}

NEO_TESTS (span_tests, test_span_cmp_cstring_00,
           test_source_file_lookup_line_00, test_source_file_lookup_line_01,
           test_source_file_lookup_position_00,
           test_source_file_lookup_position_01)
#endif
