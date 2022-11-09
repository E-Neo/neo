/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "core/string.h"

#include <stddef.h>

String
String_new ()
{
  return (String){ .data_ = Vec_char_new () };
}

String
String_from_cstr (const char *cstr)
{
  String self = String_new ();
  while (*cstr != '\0')
    {
      Vec_char_push (&self.data_, *cstr);
      cstr++;
    }
  return self;
}

String
String_from_carray (const char *carray, size_t len)
{
  String self = String_new ();
  for (size_t i = 0; i < len; i++)
    {
      Vec_char_push (&self.data_, carray[i]);
    }
  return self;
}

void
String_drop (String *self)
{
  Vec_char_drop (&self->data_);
}

size_t
String_len (const String *self)
{
  return Vec_char_len (&self->data_);
}

const char *
String_cbegin (const String *self)
{
  return Vec_char_cbegin (&self->data_);
}

const char *
String_cend (const String *self)
{
  return Vec_char_cend (&self->data_);
}

char *
String_begin (String *self)
{
  return Vec_char_begin (&self->data_);
}

char *
String_end (String *self)
{
  return Vec_char_end (&self->data_);
}
