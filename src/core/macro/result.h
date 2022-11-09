/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_CORE_MACRO_RESULT_H_
#define NEO_CORE_MACRO_RESULT_H_

#include <assert.h>
#include <stdbool.h>

#define NEO_DECL_RESULT(NT, NE, T, E)                                         \
  typedef struct Result_##NT##_##NE                                           \
  {                                                                           \
    bool is_ok_;                                                              \
    union                                                                     \
    {                                                                         \
      T ok_;                                                                  \
      E err_;                                                                 \
    };                                                                        \
  } Result_##NT##_##NE;                                                       \
  Result_##NT##_##NE Result_##NT##_##NE##_ok (T val);                         \
  Result_##NT##_##NE Result_##NT##_##NE##_err (E err);                        \
  bool Result_##NT##_##NE##_is_ok (const Result_##NT##_##NE *self);           \
  T Result_##NT##_##NE##_unwrap (Result_##NT##_##NE *self);                   \
  E Result_##NT##_##NE##_unwrap_err (Result_##NT##_##NE *self);

#define NEO_IMPL_RESULT(NT, NE, T, E)                                         \
  Result_##NT##_##NE Result_##NT##_##NE##_ok (T val)                          \
  {                                                                           \
    return (Result_##NT##_##NE){ .is_ok_ = true, .ok_ = val };                \
  }                                                                           \
                                                                              \
  Result_##NT##_##NE Result_##NT##_##NE##_err (E err)                         \
  {                                                                           \
    return (Result_##NT##_##NE){ .is_ok_ = false, .err_ = err };              \
  }                                                                           \
                                                                              \
  bool Result_##NT##_##NE##_is_ok (const Result_##NT##_##NE *self)            \
  {                                                                           \
    return self->is_ok_;                                                      \
  }                                                                           \
                                                                              \
  T Result_##NT##_##NE##_unwrap (Result_##NT##_##NE *self)                    \
  {                                                                           \
    assert (Result_##NT##_##NE##_is_ok (self));                               \
    return self->ok_;                                                         \
  }                                                                           \
                                                                              \
  E Result_##NT##_##NE##_unwrap_err (Result_##NT##_##NE *self)                \
  {                                                                           \
    assert (!Result_##NT##_##NE##_is_ok (self));                              \
    return self->err_;                                                        \
  }

#endif
