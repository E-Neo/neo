/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_TEST_H
#define NEO_TEST_H

#include <inttypes.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <threads.h>

#include "array_macro.h"
#include "string.h"
#include "vec_macro.h"

typedef struct TestFnWrapper TestFnWrapper;
typedef void (*TestFn) (TestFnWrapper *);

enum TestFnStatus
{
  TEST_NOT_RUN,
  TEST_OK,
  TEST_FAILED
};

typedef struct TestFnWrapper
{
  enum TestFnStatus status_;
  const char *file_;
  const char *func_;
  int line_;
  TestFn test_fn_;
  String message_;
  mtx_t *output_lock_;
} TestFnWrapper;

NEO_DECL_ARRAY (TestFnWrapper, TestFnWrapper)
NEO_DECL_VEC (TestFnWrapper, TestFnWrapper)

typedef Array_TestFnWrapper Tests;

typedef struct TestManager
{
  Vec_TestFnWrapper tests_;
  size_t num_threads_;
  mtx_t output_lock_;
} TestManager;

TestManager TestManager_new (size_t num_threads);
void TestManager_drop (TestManager *self);
size_t TestManager_get_total_count (const TestManager *self);
size_t TestManager_get_failed_count (const TestManager *self);
void TestManager_push (TestManager *self, TestFnWrapper test);
void TestManager_push_tests (TestManager *self, Array_TestFnWrapper tests);
void TestManager_run (TestManager *self);

#define NEO_TEST(NAME)                                                        \
  static void NAME##_test_fn_ (TestFnWrapper *test_fn_wrapper_);              \
  static const TestFnWrapper NAME = { .status_ = TEST_NOT_RUN,                \
                                      .file_ = __FILE__,                      \
                                      .func_ = #NAME,                         \
                                      .line_ = __LINE__,                      \
                                      .test_fn_ = NAME##_test_fn_ };          \
  static void NAME##_test_fn_ (TestFnWrapper *test_fn_wrapper_)

#define NEO_TESTS(NAME, ...)                                                  \
  static TestFnWrapper NAME##_begin_[] = { __VA_ARGS__ };                     \
  Tests NAME ()                                                               \
  {                                                                           \
    return (Tests){ .begin_ = NAME##_begin_,                                  \
                    .len_                                                     \
                    = sizeof (NAME##_begin_) / sizeof (TestFnWrapper) };      \
  }

#define ASSERT_I64_EQ(X, Y)                                                   \
  do                                                                          \
    {                                                                         \
      int64_t left = (X), right = (Y);                                        \
      if (left != right)                                                      \
        {                                                                     \
          test_fn_wrapper_->status_ = TEST_FAILED;                            \
          String_push_cstring (&test_fn_wrapper_->message_,                   \
                               "\e[31mFAILED\e[0m\n"                          \
                               "    assert failed: `(left == right)`\n"       \
                               "      left: `");                              \
          String_push_u64 (&test_fn_wrapper_->message_, left);                \
          String_push_cstring (&test_fn_wrapper_->message_, "`,\n");          \
          String_push_cstring (&test_fn_wrapper_->message_, "     right: `"); \
          String_push_i64 (&test_fn_wrapper_->message_, right);               \
          String_push_cstring (&test_fn_wrapper_->message_, "`, ");           \
          String_push_cstring (&test_fn_wrapper_->message_,                   \
                               test_fn_wrapper_->file_);                      \
          String_push_cstring (&test_fn_wrapper_->message_, ":");             \
          String_push_carray (&test_fn_wrapper_->message_, __func__,          \
                              strlen (__func__) - 9);                         \
          String_push_cstring (&test_fn_wrapper_->message_, ":");             \
          String_push_i64 (&test_fn_wrapper_->message_, __LINE__);            \
          String_push_cstring (&test_fn_wrapper_->message_, "\n");            \
          return;                                                             \
        }                                                                     \
    }                                                                         \
  while (0)

#define ASSERT_U64_EQ(X, Y)                                                   \
  do                                                                          \
    {                                                                         \
      uint64_t left = (X), right = (Y);                                       \
      if (left != right)                                                      \
        {                                                                     \
          test_fn_wrapper_->status_ = TEST_FAILED;                            \
          String_push_cstring (&test_fn_wrapper_->message_,                   \
                               "\e[31mFAILED\e[0m\n"                          \
                               "    assert failed: `(left == right)`\n"       \
                               "      left: `");                              \
          String_push_u64 (&test_fn_wrapper_->message_, left);                \
          String_push_cstring (&test_fn_wrapper_->message_, "`,\n");          \
          String_push_cstring (&test_fn_wrapper_->message_, "     right: `"); \
          String_push_u64 (&test_fn_wrapper_->message_, right);               \
          String_push_cstring (&test_fn_wrapper_->message_, "`, ");           \
          String_push_cstring (&test_fn_wrapper_->message_,                   \
                               test_fn_wrapper_->file_);                      \
          String_push_cstring (&test_fn_wrapper_->message_, ":");             \
          String_push_carray (&test_fn_wrapper_->message_, __func__,          \
                              strlen (__func__) - 9);                         \
          String_push_cstring (&test_fn_wrapper_->message_, ":");             \
          String_push_u64 (&test_fn_wrapper_->message_, __LINE__);            \
          String_push_cstring (&test_fn_wrapper_->message_, "\n");            \
          return;                                                             \
        }                                                                     \
    }                                                                         \
  while (0)

#endif
