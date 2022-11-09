/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "file/span.h"

#include <stddef.h>

Span
Span_new (size_t begin, size_t len)
{
  return (Span){ .begin_ = begin, .len_ = len };
}

size_t
Span_get_begin (const Span *self)
{
  return self->begin_;
}

size_t
Span_get_len (const Span *self)
{
  return self->len_;
}
