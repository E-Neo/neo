/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "string.h"

#include <inttypes.h>
#include <stddef.h>
#include <stdio.h>

#include "vec.h"

#define INT64_STRING_SIZE (32)

String
String_new ()
{
  return (String){ .data_ = Vec_char_new () };
}

String
String_from_cstring (const char *cstr)
{
  Vec_char data = Vec_char_new ();
  while (*cstr)
    {
      Vec_char_push (&data, *cstr);
      cstr++;
    }
  return (String){ .data_ = data };
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

void
String_push (String *self, char c)
{
  Vec_char_push (&self->data_, c);
}

void
String_push_repeat (String *self, char c, size_t count)
{
  for (size_t i = 0; i < count; i++)
    {
      String_push (self, c);
    }
}

void
String_push_string (String *self, const String *str)
{
  for (const char *c = String_cbegin (str); c < String_cend (str); c++)
    {
      String_push (self, *c);
    }
}

void
String_push_cstring (String *self, const char *cstr)
{
  while (*cstr)
    {
      String_push (self, *cstr);
      cstr++;
    }
}

void
String_push_cstring_repeat (String *self, const char *cstr, size_t count)
{
  for (size_t i = 0; i < count; i++)
    {
      String_push_cstring (self, cstr);
    }
}

void
String_push_carray (String *self, const char *array, size_t len)
{
  for (size_t i = 0; i < len; i++)
    {
      String_push (self, array[i]);
    }
}

void
String_push_i64 (String *self, int64_t n)
{
  char buffer[INT64_STRING_SIZE];
  snprintf (buffer, INT64_STRING_SIZE, "%" PRIi64, n);
  String_push_cstring (self, buffer);
}

void
String_push_u64 (String *self, uint64_t n)
{
  char buffer[INT64_STRING_SIZE];
  snprintf (buffer, INT64_STRING_SIZE, "%" PRIu64, n);
  String_push_cstring (self, buffer);
}

void
String_clear (String *self)
{
  Vec_char_clear (&self->data_);
}
