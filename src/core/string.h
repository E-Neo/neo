/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_CORE_STRING_H_
#define NEO_CORE_STRING_H_

#include <stddef.h>

#include "core/vec.h"

typedef struct String
{
  Vec_char data_;
} String;

String String_new ();
String String_from_cstr (const char *cstr);
String String_from_carray (const char *carray, size_t len);
void String_drop (String *self);
size_t String_len (const String *self);
const char *String_cbegin (const String *self);
const char *String_cend (const String *self);
char *String_begin (String *self);
char *String_end (String *self);

#endif
