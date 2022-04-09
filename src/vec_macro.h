/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_VEC_MACRO_H
#define NEO_VEC_MACRO_H

#include <stdbool.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>

#include "result.h"

#define NEO_DECL_VEC(N, T)                                                    \
  typedef struct Vec_##N                                                      \
  {                                                                           \
    T *begin_;                                                                \
    T *end_;                                                                  \
    T *end_cap_;                                                              \
  } Vec_##N;                                                                  \
                                                                              \
  Vec_##N Vec_##N##_with_capacity (size_t capacity);                          \
  Vec_##N Vec_##N##_new ();                                                   \
  void Vec_##N##_drop (Vec_##N *self);                                        \
  bool Vec_##N##_is_empty (const Vec_##N *self);                              \
  size_t Vec_##N##_len (const Vec_##N *self);                                 \
  size_t Vec_##N##_capacity (const Vec_##N *self);                            \
  T const *Vec_##N##_cbegin (const Vec_##N *self);                            \
  T const *Vec_##N##_cend (const Vec_##N *self);                              \
  Result_size_t_size_t Vec_##N##_binary_search_by (                           \
      const Vec_##N *self, T const *key, int (*compare) (T, T));              \
  T *Vec_##N##_begin (Vec_##N *self);                                         \
  T *Vec_##N##_end (Vec_##N *self);                                           \
  void Vec_##N##_reserve (Vec_##N *self, size_t additional);                  \
  void Vec_##N##_push_uninit (Vec_##N *self);                                 \
  void Vec_##N##_push (Vec_##N *self, T value);                               \
  T Vec_##N##_pop (Vec_##N *self);                                            \
  void Vec_##N##_resize (Vec_##N *self, size_t new_len, T value);             \
  void Vec_##N##_clear (Vec_##N *self);

#define NEO_IMPL_VEC(N, T)                                                    \
  Vec_##N Vec_##N##_with_capacity (size_t capacity)                           \
  {                                                                           \
    if (capacity == 0)                                                        \
      {                                                                       \
        return (Vec_##N){ .begin_ = NULL, .end_ = NULL, .end_cap_ = NULL };   \
      }                                                                       \
    T *begin = (T *)malloc (sizeof (T) * capacity);                           \
    if (begin == NULL)                                                        \
      {                                                                       \
        abort ();                                                             \
      }                                                                       \
    return (Vec_##N){ .begin_ = begin,                                        \
                      .end_ = begin,                                          \
                      .end_cap_ = begin + capacity };                         \
  }                                                                           \
                                                                              \
  Vec_##N Vec_##N##_new () { return Vec_##N##_with_capacity (0); }            \
                                                                              \
  void Vec_##N##_drop (Vec_##N *self)                                         \
  {                                                                           \
    free (self->begin_);                                                      \
    self->begin_ = NULL;                                                      \
    self->end_ = NULL;                                                        \
    self->end_cap_ = NULL;                                                    \
  }                                                                           \
                                                                              \
  bool Vec_##N##_is_empty (const Vec_##N *self)                               \
  {                                                                           \
    return self->end_ == self->begin_;                                        \
  }                                                                           \
                                                                              \
  size_t Vec_##N##_len (const Vec_##N *self)                                  \
  {                                                                           \
    return self->end_ - self->begin_;                                         \
  }                                                                           \
                                                                              \
  size_t Vec_##N##_capacity (const Vec_##N *self)                             \
  {                                                                           \
    return self->end_cap_ - self->begin_;                                     \
  }                                                                           \
                                                                              \
  T const *Vec_##N##_cbegin (const Vec_##N *self) { return self->begin_; }    \
                                                                              \
  T const *Vec_##N##_cend (const Vec_##N *self) { return self->end_; }        \
                                                                              \
  Result_size_t_size_t Vec_##N##_binary_search_by (                           \
      const Vec_##N *self, T const *key, int (*compare) (T, T))               \
  {                                                                           \
    T const *lo = Vec_##N##_cbegin (self);                                    \
    T const *hi = Vec_##N##_cend (self);                                      \
    while (lo < hi)                                                           \
      {                                                                       \
        T const *mi = lo + (hi - lo) / 2;                                     \
        if (compare (*key, *mi) > 0)                                          \
          {                                                                   \
            lo = mi + 1;                                                      \
          }                                                                   \
        else if (compare (*key, *mi) < 0)                                     \
          {                                                                   \
            hi = mi;                                                          \
          }                                                                   \
        else                                                                  \
          {                                                                   \
            return Result_size_t_size_t_ok (mi - Vec_##N##_cbegin (self));    \
          }                                                                   \
      }                                                                       \
    return Result_size_t_size_t_err (lo - Vec_##N##_cbegin (self));           \
  }                                                                           \
                                                                              \
  T *Vec_##N##_begin (Vec_##N *self) { return self->begin_; }                 \
                                                                              \
  T *Vec_##N##_end (Vec_##N *self) { return self->end_; }                     \
                                                                              \
  void Vec_##N##_reserve (Vec_##N *self, size_t additional)                   \
  {                                                                           \
    size_t len = Vec_##N##_len (self);                                        \
    size_t cap = Vec_##N##_capacity (self);                                   \
    if (len + additional <= cap)                                              \
      {                                                                       \
        return;                                                               \
      }                                                                       \
    size_t new_cap = cap ? 2 * cap : 8;                                       \
    while (len + additional > new_cap)                                        \
      {                                                                       \
        new_cap *= 2;                                                         \
      }                                                                       \
    self->begin_ = (T *)realloc (self->begin_, sizeof (T) * new_cap);         \
    if (self->begin_ == NULL)                                                 \
      {                                                                       \
        abort ();                                                             \
      }                                                                       \
    self->end_ = self->begin_ + len;                                          \
    self->end_cap_ = self->begin_ + new_cap;                                  \
  }                                                                           \
                                                                              \
  void Vec_##N##_push_uninit (Vec_##N *self)                                  \
  {                                                                           \
    Vec_##N##_reserve (self, 1);                                              \
    self->end_++;                                                             \
  }                                                                           \
                                                                              \
  void Vec_##N##_push (Vec_##N *self, T value)                                \
  {                                                                           \
    Vec_##N##_reserve (self, 1);                                              \
    *self->end_ = value;                                                      \
    self->end_++;                                                             \
  }                                                                           \
                                                                              \
  T Vec_##N##_pop (Vec_##N *self)                                             \
  {                                                                           \
    self->end_--;                                                             \
    return *self->end_;                                                       \
  }                                                                           \
                                                                              \
  void Vec_##N##_resize (Vec_##N *self, size_t new_len, T value)              \
  {                                                                           \
    size_t old_len = Vec_##N##_len (self);                                    \
    if (new_len <= old_len)                                                   \
      {                                                                       \
        return;                                                               \
      }                                                                       \
    Vec_##N##_reserve (self, new_len - old_len);                              \
    for (size_t i = 0; i < new_len - old_len; i++)                            \
      {                                                                       \
        Vec_##N##_push (self, value);                                         \
      }                                                                       \
  }                                                                           \
                                                                              \
  void Vec_##N##_clear (Vec_##N *self) { self->end_ = self->begin_; }

#endif
