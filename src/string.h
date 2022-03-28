/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_STRING_H
#define NEO_STRING_H

#include "vec.h"
#include <stddef.h>
#include <stdint.h>

typedef struct String
{
  Vec_char data_;
} String;

String String_new ();
String String_from_cstring (const char *cstr);
void String_drop (String *self);
size_t String_len (const String *self);
const char *String_cbegin (const String *self);
const char *String_cend (const String *self);
char *String_begin (String *self);
char *String_end (String *self);
void String_push (String *self, char c);
void String_push_repeat (String *self, char c, size_t count);
void String_push_string (String *self, const String *str);
void String_push_cstring (String *self, const char *cstr);
void String_push_cstring_repeat (String *self, const char *cstr, size_t count);
void String_push_carray (String *self, const char *array, size_t len);
void String_push_i64 (String *self, int64_t n);
void String_push_u64 (String *self, uint64_t n);
void String_clear (String *self);

#endif
