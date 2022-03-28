/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_QUEUE_MACRO_H
#define NEO_QUEUE_MACRO_H

#include <stdbool.h>
#include <stddef.h>

#include "vec.h"

#define NEO_DECL_QUEUE(N, T)                                                  \
  typedef struct Queue_##N                                                    \
  {                                                                           \
    Vec_##N data_;                                                            \
    size_t begin_;                                                            \
    size_t end_;                                                              \
  } Queue_##N;                                                                \
                                                                              \
  Queue_##N Queue_##N##_new ();                                               \
  void Queue_##N##_drop (Queue_##N *self);                                    \
  bool Queue_##N##_is_empty (const Queue_##N *self);                          \
  size_t Queue_##N##_len (const Queue_##N *self);                             \
  const T *Queue_##N##_cbegin (const Queue_##N *self);                        \
  const T *Queue_##N##_cend (const Queue_##N *self);                          \
  const T *Queue_##N##_cnext (const Queue_##N *self, const T *ptr);           \
  T *Queue_##N##_begin (Queue_##N *self);                                     \
  T *Queue_##N##_end (Queue_##N *self);                                       \
  T *Queue_##N##_next (Queue_##N *self, T *ptr);                              \
  void Queue_##N##_reserve (Queue_##N *self, size_t additional);              \
  void Queue_##N##_push_back (Queue_##N *self, T value);                      \
  T Queue_##N##_pop_back (Queue_##N *self);                                   \
  void Queue_##N##_push_front (Queue_##N *self, T value);                     \
  T Queue_##N##_pop_front (Queue_##N *self);

#define NEO_IMPL_QUEUE(N, T)                                                  \
                                                                              \
  Queue_##N Queue_##N##_new ()                                                \
  {                                                                           \
    return (Queue_##N){ .data_ = Vec_##N##_new (), .begin_ = 0, .end_ = 0 };  \
  }                                                                           \
                                                                              \
  void Queue_##N##_drop (Queue_##N *self) { Vec_##N##_drop (&self->data_); }  \
                                                                              \
  size_t Queue_##N##_len (const Queue_##N *self)                              \
  {                                                                           \
    if (self->end_ < self->begin_)                                            \
      {                                                                       \
        return self->end_ + Vec_##N##_capacity (&self->data_) - self->begin_; \
      }                                                                       \
    return self->end_ - self->begin_;                                         \
  }                                                                           \
                                                                              \
  bool Queue_##N##_is_empty (const Queue_##N *self)                           \
  {                                                                           \
    return Queue_##N##_len (self) == 0;                                       \
  }                                                                           \
                                                                              \
  const T *Queue_##N##_cbegin (const Queue_##N *self)                         \
  {                                                                           \
    return Vec_##N##_cbegin (&self->data_) + self->begin_;                    \
  }                                                                           \
                                                                              \
  const T *Queue_##N##_cend (const Queue_##N *self)                           \
  {                                                                           \
    return Vec_##N##_cbegin (&self->data_) + self->end_;                      \
  }                                                                           \
                                                                              \
  const T *Queue_##N##_cnext (const Queue_##N *self, const T *ptr)            \
  {                                                                           \
    if (ptr + 1                                                               \
        == Vec_##N##_cbegin (&self->data_)                                    \
               + Vec_##N##_capacity (&self->data_))                           \
      {                                                                       \
        return Vec_##N##_cbegin (&self->data_);                               \
      }                                                                       \
    return ptr + 1;                                                           \
  }                                                                           \
                                                                              \
  T *Queue_##N##_begin (Queue_##N *self)                                      \
  {                                                                           \
    return Vec_##N##_begin (&self->data_) + self->begin_;                     \
  }                                                                           \
                                                                              \
  T *Queue_##N##_end (Queue_##N *self)                                        \
  {                                                                           \
    return Vec_##N##_begin (&self->data_) + self->end_;                       \
  }                                                                           \
                                                                              \
  T *Queue_##N##_next (Queue_##N *self, T *ptr)                               \
  {                                                                           \
    if (ptr + 1                                                               \
        == Vec_##N##_cbegin (&self->data_)                                    \
               + Vec_##N##_capacity (&self->data_))                           \
      {                                                                       \
        return Vec_##N##_begin (&self->data_);                                \
      }                                                                       \
    return ptr + 1;                                                           \
  }                                                                           \
                                                                              \
  void Queue_##N##_reserve (Queue_##N *self, size_t additional)               \
  {                                                                           \
    size_t len = Queue_##N##_len (self);                                      \
    size_t cap = Vec_##N##_capacity (&self->data_);                           \
    if (len + additional + 1 <= cap)                                          \
      {                                                                       \
        return;                                                               \
      }                                                                       \
    size_t new_cap = cap ? cap + additional : cap + additional + 1;           \
    Vec_##N##_reserve (&self->data_, new_cap);                                \
    if (self->end_ >= self->begin_)                                           \
      {                                                                       \
        return;                                                               \
      }                                                                       \
    size_t new_end = cap;                                                     \
    for (size_t i = 0; i < self->end_; i++)                                   \
      {                                                                       \
        Vec_##N##_begin (&self->data_)[new_end]                               \
            = Vec_##N##_begin (&self->data_)[i];                              \
        new_end = (new_end + 1) % Vec_##N##_capacity (&self->data_);          \
      }                                                                       \
    self->end_ = new_end;                                                     \
  }                                                                           \
                                                                              \
  void Queue_##N##_push_back (Queue_##N *self, T value)                       \
  {                                                                           \
    Queue_##N##_reserve (self, 1);                                            \
    Vec_##N##_begin (&self->data_)[self->end_] = value;                       \
    self->end_ = (self->end_ + 1) % Vec_##N##_capacity (&self->data_);        \
  }                                                                           \
                                                                              \
  T Queue_##N##_pop_back (Queue_##N *self)                                    \
  {                                                                           \
    size_t new_end = self->end_ ? self->end_ - 1                              \
                                : Vec_##N##_capacity (&self->data_) - 1;      \
    self->end_ = new_end;                                                     \
    return Vec_##N##_begin (&self->data_)[new_end];                           \
  }                                                                           \
                                                                              \
  void Queue_##N##_push_front (Queue_##N *self, T value)                      \
  {                                                                           \
    Queue_##N##_reserve (self, 1);                                            \
    size_t new_begin = self->begin_ ? self->begin_ - 1                        \
                                    : Vec_##N##_capacity (&self->data_) - 1;  \
    Vec_##N##_begin (&self->data_)[new_begin] = value;                        \
    self->begin_ = new_begin;                                                 \
  }                                                                           \
                                                                              \
  T Queue_##N##_pop_front (Queue_##N *self)                                   \
  {                                                                           \
    size_t begin = self->begin_;                                              \
    self->begin_ = (self->begin_ + 1) % Vec_##N##_capacity (&self->data_);    \
    return Vec_##N##_begin (&self->data_)[begin];                             \
  }

#endif
