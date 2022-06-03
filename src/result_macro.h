/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_RESULT_MACRO_H
#define NEO_RESULT_MACRO_H

#include <assert.h>
#include <stdbool.h>

enum ResultKind
{
  RESULT_OK,
  RESULT_ERR
};

#define NEO_DECL_RESULT(NT, NE, T, E)                                         \
  typedef struct Result_##NT##_##NE                                           \
  {                                                                           \
    enum ResultKind kind_;                                                    \
    union                                                                     \
    {                                                                         \
      T ok_value_;                                                            \
      E err_value_;                                                           \
    };                                                                        \
  } Result_##NT##_##NE;                                                       \
                                                                              \
  Result_##NT##_##NE Result_##NT##_##NE##_ok (T ok_value);                    \
  Result_##NT##_##NE Result_##NT##_##NE##_err (E err_value);                  \
  bool Result_##NT##_##NE##_is_ok (const Result_##NT##_##NE *self);           \
  T Result_##NT##_##NE##_unwrap (const Result_##NT##_##NE *self);             \
  E Result_##NT##_##NE##_unwrap_err (const Result_##NT##_##NE *self);

#define NEO_IMPL_RESULT(NT, NE, T, E)                                         \
  Result_##NT##_##NE Result_##NT##_##NE##_ok (T ok_value)                     \
  {                                                                           \
    return (Result_##NT##_##NE){ .kind_ = RESULT_OK, .ok_value_ = ok_value }; \
  }                                                                           \
                                                                              \
  Result_##NT##_##NE Result_##NT##_##NE##_err (E err_value)                   \
  {                                                                           \
    return (Result_##NT##_##NE){ .kind_ = RESULT_ERR,                         \
                                 .err_value_ = err_value };                   \
  }                                                                           \
                                                                              \
  bool Result_##NT##_##NE##_is_ok (const Result_##NT##_##NE *self)            \
  {                                                                           \
    return self->kind_ == RESULT_OK;                                          \
  }                                                                           \
                                                                              \
  T Result_##NT##_##NE##_unwrap (const Result_##NT##_##NE *self)              \
  {                                                                           \
    assert (self->kind_ == RESULT_OK);                                        \
    return self->ok_value_;                                                   \
  }                                                                           \
                                                                              \
  E Result_##NT##_##NE##_unwrap_err (const Result_##NT##_##NE *self)          \
  {                                                                           \
    assert (self->kind_ == RESULT_ERR);                                       \
    return self->err_value_;                                                  \
  }

#endif
