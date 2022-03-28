/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_OPTION_MACRO_H
#define NEO_OPTION_MACRO_H

#include <stdbool.h>

enum OptionKind
{
  OPTION_SOME,
  OPTION_NONE
};

#define NEO_DECL_OPTION(N, T)                                                 \
  typedef struct Option_##N                                                   \
  {                                                                           \
    enum OptionKind kind_;                                                    \
    T value_;                                                                 \
  } Option_##N;                                                               \
                                                                              \
  Option_##N Option_##N##_some (T value);                                     \
  Option_##N Option_##N##_none ();                                            \
  bool Option_##N##_is_some (const Option_##N *self);                         \
  T Option_##N##_unwrap (const Option_##N *self);

#define NEO_IMPL_OPTION(N, T)                                                 \
  Option_##N Option_##N##_some (T value)                                      \
  {                                                                           \
    return (Option_##N){ .kind_ = OPTION_SOME, .value_ = value };             \
  }                                                                           \
                                                                              \
  Option_##N Option_##N##_none ()                                             \
  {                                                                           \
    return (Option_##N){ .kind_ = OPTION_NONE };                              \
  }                                                                           \
                                                                              \
  bool Option_##N##_is_some (const Option_##N *self)                          \
  {                                                                           \
    return self->kind_ == OPTION_SOME;                                        \
  }                                                                           \
                                                                              \
  T Option_##N##_unwrap (const Option_##N *self) { return self->value_; }

#endif
