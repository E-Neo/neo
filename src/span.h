/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_SPAN_H
#define NEO_SPAN_H

#include <stddef.h>

#include "string.h"
#include "vec.h"

typedef struct Span
{
  const char *begin_;
  const char *end_;
} Span;

Span Span_new (const char *begin, size_t len);
Span Span_from_string (const String *str);
Span Span_from_cstring (const char *str);
const char *Span_cbegin (const Span *self);
const char *Span_cend (const Span *self);
size_t Span_len (const Span *self);
int Span_cmp_cstring (const Span *self, const char *cstr);

typedef struct Position
{
  size_t line_;   /* 1-based. */
  size_t column_; /* 0-based. */
} Position;

Position Position_new (size_t line, size_t column);
size_t Position_get_line (const Position *self);
size_t Position_get_column (const Position *self);

typedef struct SourceFile
{
  String path_;
  String content_;
  Vec_const_char_ptr lines_;
} SourceFile;

SourceFile SourceFile_new (String path, String content);
void SourceFile_drop (SourceFile *self);
const String *SourceFile_get_path (const SourceFile *self);
const String *SourceFile_get_content (const SourceFile *self);
Position SourceFile_lookup_position (const SourceFile *self, const char *pos);
Span SourceFile_get_line (const SourceFile *self, size_t line);

#ifdef TESTS
#include "test.h"
Tests span_tests ();
#endif

#endif
