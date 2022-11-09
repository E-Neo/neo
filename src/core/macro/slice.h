/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_CORE_MACRO_SLICE_H_
#define NEO_CORE_MACRO_SLICE_H_

#include <stdbool.h>
#include <stddef.h>

#include "core/result.h"

#define NEO_DECL_SLICE(N, T)                                                  \
  typedef struct Slice_##N                                                    \
  {                                                                           \
    T *begin_;                                                                \
    size_t len_;                                                              \
  } Slice_##N;                                                                \
                                                                              \
  Slice_##N Slice_##N##_new (T *begin, size_t len);                           \
  bool Slice_##N##_is_empty (const Slice_##N *self);                          \
  size_t Slice_##N##_len (const Slice_##N *self);                             \
  T const *Slice_##N##_cbegin (const Slice_##N *self);                        \
  T const *Slice_##N##_cend (const Slice_##N *self);                          \
  T *Slice_##N##_begin (Slice_##N *self);                                     \
  T *Slice_##N##_end (Slice_##N *self);                                       \
  Result_size_t_size_t Slice_##N##_binary_search (                            \
      const Slice_##N *self, T const *key,                                    \
      int (*cmp) (T const *, T const *));

#define NEO_IMPL_SLICE(N, T)                                                  \
  Slice_##N Slice_##N##_new (T *begin, size_t len)                            \
  {                                                                           \
    return (Slice_##N){ .begin_ = begin, .len_ = len };                       \
  }                                                                           \
                                                                              \
  bool Slice_##N##_is_empty (const Slice_##N *self)                           \
  {                                                                           \
    return self->len_ == 0;                                                   \
  }                                                                           \
                                                                              \
  size_t Slice_##N##_len (const Slice_##N *self) { return self->len_; }       \
                                                                              \
  T const *Slice_##N##_cbegin (const Slice_##N *self)                         \
  {                                                                           \
    return self->begin_;                                                      \
  }                                                                           \
                                                                              \
  T const *Slice_##N##_cend (const Slice_##N *self)                           \
  {                                                                           \
    return self->begin_ + self->len_;                                         \
  }                                                                           \
                                                                              \
  T *Slice_##N##_begin (Slice_##N *self) { return self->begin_; }             \
                                                                              \
  T *Slice_##N##_end (Slice_##N *self) { return self->begin_ + self->len_; }  \
                                                                              \
  Result_size_t_size_t Slice_##N##_binary_search (                            \
      const Slice_##N *self, T const *key, int (*cmp) (T const *, T const *)) \
  {                                                                           \
    T const *cbegin = Slice_##N##_cbegin (self);                              \
    size_t lo = 0;                                                            \
    size_t hi = Slice_##N##_len (self);                                       \
    while (lo < hi)                                                           \
      {                                                                       \
        size_t mi = lo + (hi - lo) / 2;                                       \
        int cmp_res = cmp (key, cbegin + mi);                                 \
        if (cmp_res == 0)                                                     \
          {                                                                   \
            return Result_size_t_size_t_ok (mi);                              \
          }                                                                   \
        else if (cmp_res > 0)                                                 \
          {                                                                   \
            lo = mi + 1;                                                      \
          }                                                                   \
        else                                                                  \
          {                                                                   \
            hi = mi;                                                          \
          }                                                                   \
      }                                                                       \
    return Result_size_t_size_t_err (lo);                                     \
  }

#endif
