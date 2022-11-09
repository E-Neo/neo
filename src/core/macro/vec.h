/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_CORE_MACRO_VEC_H_
#define NEO_CORE_MACRO_VEC_H_

#include <assert.h>
#include <stdbool.h>
#include <stddef.h>

#include "core/alloc.h"

#define NEO_DECL_VEC(N, T)                                                    \
  typedef struct Vec_##N                                                      \
  {                                                                           \
    T *begin_;                                                                \
    size_t len_;                                                              \
    size_t cap_;                                                              \
  } Vec_##N;                                                                  \
                                                                              \
  Vec_##N Vec_##N##_new ();                                                   \
  Vec_##N Vec_##N##_with_capacity (size_t capacity);                          \
  void Vec_##N##_drop (Vec_##N *self);                                        \
  bool Vec_##N##_is_empty (const Vec_##N *self);                              \
  size_t Vec_##N##_len (const Vec_##N *self);                                 \
  size_t Vec_##N##_capacity (const Vec_##N *self);                            \
  T const *Vec_##N##_cbegin (const Vec_##N *self);                            \
  T const *Vec_##N##_cend (const Vec_##N *self);                              \
  T *Vec_##N##_begin (Vec_##N *self);                                         \
  T *Vec_##N##_end (Vec_##N *self);                                           \
  void Vec_##N##_reserve (Vec_##N *self, size_t additional);                  \
  void Vec_##N##_push_uninit (Vec_##N *self);                                 \
  void Vec_##N##_push (Vec_##N *self, T value);                               \
  T Vec_##N##_pop (Vec_##N *self);                                            \
  void Vec_##N##_clear (Vec_##N *self);

#define NEO_IMPL_VEC(N, T)                                                    \
  Vec_##N Vec_##N##_new () { return Vec_##N##_with_capacity (0); }            \
                                                                              \
  Vec_##N Vec_##N##_with_capacity (size_t capacity)                           \
  {                                                                           \
    Vec_##N res = (Vec_##N){ .begin_ = NULL, .len_ = 0, .cap_ = 0 };          \
    Vec_##N##_reserve (&res, capacity);                                       \
    return res;                                                               \
  }                                                                           \
                                                                              \
  void Vec_##N##_drop (Vec_##N *self)                                         \
  {                                                                           \
    neo_free (self->begin_);                                                  \
    self->begin_ = NULL;                                                      \
    self->len_ = 0;                                                           \
    self->cap_ = 0;                                                           \
  }                                                                           \
                                                                              \
  bool Vec_##N##_is_empty (const Vec_##N *self) { return self->len_ == 0; }   \
                                                                              \
  size_t Vec_##N##_len (const Vec_##N *self) { return self->len_; }           \
                                                                              \
  size_t Vec_##N##_capacity (const Vec_##N *self) { return self->cap_; }      \
                                                                              \
  T const *Vec_##N##_cbegin (const Vec_##N *self) { return self->begin_; }    \
                                                                              \
  T const *Vec_##N##_cend (const Vec_##N *self)                               \
  {                                                                           \
    return self->begin_ + self->len_;                                         \
  }                                                                           \
                                                                              \
  T *Vec_##N##_begin (Vec_##N *self) { return self->begin_; }                 \
                                                                              \
  T *Vec_##N##_end (Vec_##N *self) { return self->begin_ + self->len_; }      \
                                                                              \
  void Vec_##N##_reserve (Vec_##N *self, size_t additional)                   \
  {                                                                           \
    size_t len = self->len_;                                                  \
    size_t cap = self->cap_;                                                  \
    if (len + additional <= cap)                                              \
      {                                                                       \
        return;                                                               \
      }                                                                       \
    size_t new_cap = cap == 0 ? 1 : 2 * cap;                                  \
    while (len + additional > new_cap)                                        \
      {                                                                       \
        new_cap *= 2;                                                         \
      }                                                                       \
    self->begin_ = (T *)neo_realloc (self->begin_, sizeof (T) * new_cap);     \
    self->len_ = len;                                                         \
    self->cap_ = new_cap;                                                     \
  }                                                                           \
                                                                              \
  void Vec_##N##_push_uninit (Vec_##N *self)                                  \
  {                                                                           \
    Vec_##N##_reserve (self, 1);                                              \
    self->len_++;                                                             \
  }                                                                           \
                                                                              \
  void Vec_##N##_push (Vec_##N *self, T value)                                \
  {                                                                           \
    Vec_##N##_reserve (self, 1);                                              \
    self->begin_[self->len_] = value;                                         \
    self->len_++;                                                             \
  }                                                                           \
                                                                              \
  T Vec_##N##_pop (Vec_##N *self)                                             \
  {                                                                           \
    assert (!Vec_##N##_is_empty (self));                                      \
    self->len_--;                                                             \
    return self->begin_[self->len_];                                          \
  }                                                                           \
                                                                              \
  void Vec_##N##_clear (Vec_##N *self) { self->len_ = 0; }

#endif
