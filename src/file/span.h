/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_FILE_SPAN_H_
#define NEO_FILE_SPAN_H_

#include <stddef.h>

typedef struct Span
{
  size_t begin_;
  size_t len_;
} Span;

Span Span_new (size_t begin, size_t len);
size_t Span_get_begin (const Span *self);
size_t Span_get_len (const Span *self);

#endif
