/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_ARRAY_MACRO_H
#define NEO_ARRAY_MACRO_H

#include <stdbool.h>
#include <stddef.h>

#define NEO_DECL_ARRAY(N, T)                                                  \
  typedef struct Array_##N                                                    \
  {                                                                           \
    T *begin_;                                                                \
    size_t len_;                                                              \
  } Array_##N;                                                                \
                                                                              \
  Array_##N Array_##N##_new (T *begin, size_t len);                           \
  bool Array_##N##_is_empty (const Array_##N *self);                          \
  size_t Array_##N##_len (const Array_##N *self);                             \
  T const *Array_##N##_cbegin (const Array_##N *self);                        \
  T const *Array_##N##_cend (const Array_##N *self);                          \
  T *Array_##N##_begin (Array_##N *self);                                     \
  T *Array_##N##_end (Array_##N *self);

#define NEO_IMPL_ARRAY(N, T)                                                  \
  Array_##N Array_##N##_new (T *begin, size_t len)                            \
  {                                                                           \
    return (Array_##N){ .begin_ = begin, .len_ = len };                       \
  }                                                                           \
                                                                              \
  bool Array_##N##_is_empty (const Array_##N *self)                           \
  {                                                                           \
    return self->len_ == 0;                                                   \
  }                                                                           \
                                                                              \
  size_t Array_##N##_len (const Array_##N *self) { return self->len_; }       \
                                                                              \
  T const *Array_##N##_cbegin (const Array_##N *self)                         \
  {                                                                           \
    return self->begin_;                                                      \
  }                                                                           \
                                                                              \
  T const *Array_##N##_cend (const Array_##N *self)                           \
  {                                                                           \
    return self->begin_ + self->len_;                                         \
  }                                                                           \
                                                                              \
  T *Array_##N##_begin (Array_##N *self) { return self->begin_; }             \
                                                                              \
  T *Array_##N##_end (Array_##N *self) { return self->begin_ + self->len_; }

#endif
