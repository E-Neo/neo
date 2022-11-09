/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_CORE_MACRO_QUEUE_H_
#define NEO_CORE_MACRO_QUEUE_H_

#include <assert.h>
#include <stdbool.h>
#include <stddef.h>
#include <string.h>

#include "core/alloc.h"

#define NEO_DECL_QUEUE(N, T)                                                  \
  typedef struct Queue_##N                                                    \
  {                                                                           \
    T *begin_;                                                                \
    size_t cap_;                                                              \
    size_t head_; /* the head of the queue that is ready to be read */        \
    size_t tail_; /* the tail of the queue that is ready to be written */     \
  } Queue_##N;                                                                \
                                                                              \
  Queue_##N Queue_##N##_new ();                                               \
  Queue_##N Queue_##N##_with_capacity (size_t capacity);                      \
  void Queue_##N##_drop (Queue_##N *self);                                    \
  bool Queue_##N##_is_empty (const Queue_##N *self);                          \
  size_t Queue_##N##_len (const Queue_##N *self);                             \
  void Queue_##N##_reserve (Queue_##N *self, size_t additional);              \
  T const *Queue_##N##_cbegin (const Queue_##N *self);                        \
  T const *Queue_##N##_cend (const Queue_##N *self);                          \
  T const *Queue_##N##_cnext (const Queue_##N *self, T const *ptr);           \
  T *Queue_##N##_begin (Queue_##N *self);                                     \
  T *Queue_##N##_end (Queue_##N *self);                                       \
  T *Queue_##N##_next (Queue_##N *self, T *ptr);                              \
  T *Queue_##N##_at (Queue_##N *self, size_t index);                          \
  void Queue_##N##_push_back (Queue_##N *self, T value);                      \
  T Queue_##N##_pop_back (Queue_##N *self);                                   \
  void Queue_##N##_push_front (Queue_##N *self, T value);                     \
  T Queue_##N##_pop_front (Queue_##N *self);

#define NEO_IMPL_QUEUE(N, T)                                                  \
  Queue_##N Queue_##N##_new ()                                                \
  {                                                                           \
    return (Queue_##N){ .begin_ = NULL, .cap_ = 0, .head_ = 0, .tail_ = 0 };  \
  }                                                                           \
                                                                              \
  Queue_##N Queue_##N##_with_capacity (size_t capacity)                       \
  {                                                                           \
    Queue_##N res = Queue_##N##_new ();                                       \
    Queue_##N##_reserve (&res, capacity);                                     \
    return res;                                                               \
  }                                                                           \
                                                                              \
  void Queue_##N##_drop (Queue_##N *self)                                     \
  {                                                                           \
    neo_free (self->begin_);                                                  \
    self->begin_ = NULL;                                                      \
    self->cap_ = 0;                                                           \
    self->head_ = 0;                                                          \
    self->tail_ = 0;                                                          \
  }                                                                           \
                                                                              \
  bool Queue_##N##_is_empty (const Queue_##N *self)                           \
  {                                                                           \
    return self->head_ == self->tail_;                                        \
  }                                                                           \
                                                                              \
  size_t Queue_##N##_len (const Queue_##N *self)                              \
  {                                                                           \
    size_t cap = self->cap_;                                                  \
    size_t head = self->head_;                                                \
    size_t tail = self->tail_;                                                \
    return tail >= head ? tail - head : cap - head + tail;                    \
  }                                                                           \
                                                                              \
  void Queue_##N##_reserve (Queue_##N *self, size_t additional)               \
  {                                                                           \
    size_t cap = self->cap_;                                                  \
    size_t len = Queue_##N##_len (self);                                      \
    if (len + additional < cap)                                               \
      {                                                                       \
        return;                                                               \
      }                                                                       \
    size_t new_cap = cap == 0 ? 2 : 2 * cap;                                  \
    while (len + additional >= new_cap)                                       \
      {                                                                       \
        new_cap *= 2;                                                         \
      }                                                                       \
    self->begin_ = (T *)neo_realloc (self->begin_, sizeof (T) * new_cap);     \
    self->cap_ = new_cap;                                                     \
    size_t head = self->head_;                                                \
    if (head > self->tail_)                                                   \
      {                                                                       \
        self->head_ = new_cap - (cap - head);                                 \
        memmove (self->begin_ + self->head_, self->begin_ + head,             \
                 sizeof (T) * (cap - head));                                  \
      }                                                                       \
  }                                                                           \
                                                                              \
  T const *Queue_##N##_cbegin (const Queue_##N *self)                         \
  {                                                                           \
    return self->begin_ + self->head_;                                        \
  }                                                                           \
                                                                              \
  T const *Queue_##N##_cend (const Queue_##N *self)                           \
  {                                                                           \
    return self->begin_ + self->tail_;                                        \
  }                                                                           \
                                                                              \
  T const *Queue_##N##_cnext (const Queue_##N *self, T const *ptr)            \
  {                                                                           \
    if (ptr + 1 == self->begin_ + self->cap_)                                 \
      {                                                                       \
        return self->begin_;                                                  \
      }                                                                       \
    return ptr + 1;                                                           \
  }                                                                           \
                                                                              \
  T *Queue_##N##_begin (Queue_##N *self)                                      \
  {                                                                           \
    return self->begin_ + self->head_;                                        \
  }                                                                           \
                                                                              \
  T *Queue_##N##_end (Queue_##N *self) { return self->begin_ + self->tail_; } \
                                                                              \
  T *Queue_##N##_next (Queue_##N *self, T *ptr)                               \
  {                                                                           \
    if (ptr + 1 == self->begin_ + self->cap_)                                 \
      {                                                                       \
        return self->begin_;                                                  \
      }                                                                       \
    return ptr + 1;                                                           \
  }                                                                           \
                                                                              \
  T *Queue_##N##_at (Queue_##N *self, size_t index)                           \
  {                                                                           \
    assert (index < Queue_##N##_len (self));                                  \
    size_t right_len = self->cap_ - self->head_;                              \
    if (right_len > index)                                                    \
      {                                                                       \
        return self->begin_ + self->head_ + index;                            \
      }                                                                       \
    return self->begin_ + (index - right_len);                                \
  }                                                                           \
                                                                              \
  void Queue_##N##_push_back (Queue_##N *self, T value)                       \
  {                                                                           \
    Queue_##N##_reserve (self, 1);                                            \
    self->begin_[self->tail_] = value;                                        \
    if (self->tail_ < self->cap_ - 1)                                         \
      {                                                                       \
        self->tail_++;                                                        \
      }                                                                       \
    else                                                                      \
      {                                                                       \
        self->tail_ = 0;                                                      \
      }                                                                       \
  }                                                                           \
                                                                              \
  T Queue_##N##_pop_back (Queue_##N *self)                                    \
  {                                                                           \
    assert (!Queue_##N##_is_empty (self));                                    \
    if (self->tail_ == 0)                                                     \
      {                                                                       \
        self->tail_ = self->cap_ - 1;                                         \
      }                                                                       \
    else                                                                      \
      {                                                                       \
        self->tail_--;                                                        \
      }                                                                       \
    return self->begin_[self->tail_];                                         \
  }                                                                           \
                                                                              \
  void Queue_##N##_push_front (Queue_##N *self, T value)                      \
  {                                                                           \
    Queue_##N##_reserve (self, 1);                                            \
    if (self->head_ == 0)                                                     \
      {                                                                       \
        self->head_ = self->cap_ - 1;                                         \
      }                                                                       \
    else                                                                      \
      {                                                                       \
        self->head_--;                                                        \
      }                                                                       \
    self->begin_[self->head_] = value;                                        \
  }                                                                           \
                                                                              \
  T Queue_##N##_pop_front (Queue_##N *self)                                   \
  {                                                                           \
    assert (!Queue_##N##_is_empty (self));                                    \
    size_t head = self->head_;                                                \
    if (head == self->cap_ - 1)                                               \
      {                                                                       \
        self->head_ = 0;                                                      \
      }                                                                       \
    else                                                                      \
      {                                                                       \
        self->head_++;                                                        \
      }                                                                       \
    return self->begin_[head];                                                \
  }

#endif
