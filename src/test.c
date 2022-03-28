/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "test.h"

#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <threads.h>

#include "array_macro.h"
#include "string.h"
#include "thread_pool.h"
#include "vec.h"
#include "vec_macro.h"

NEO_IMPL_ARRAY (TestFnWrapper, TestFnWrapper)
NEO_IMPL_VEC (TestFnWrapper, TestFnWrapper)

static void
TestFnWrapper_lock (TestFnWrapper *self)
{
  if (mtx_lock (self->output_lock_) != thrd_success)
    {
      abort ();
    }
}

static void
TestFnWrapper_unlock (TestFnWrapper *self)
{
  if (mtx_unlock (self->output_lock_) != thrd_success)
    {
      abort ();
    }
}

TestManager
TestManager_new (size_t num_threads)
{
  TestManager self
      = { .tests_ = Vec_TestFnWrapper_new (), .num_threads_ = num_threads };
  if (mtx_init (&self.output_lock_, mtx_plain) != thrd_success)
    {
      abort ();
    }
  return self;
}

void
TestManager_drop (TestManager *self)
{
  for (TestFnWrapper *test = Vec_TestFnWrapper_begin (&self->tests_);
       test < Vec_TestFnWrapper_end (&self->tests_); test++)
    {
      String_drop (&test->message_);
    }
  Vec_TestFnWrapper_drop (&self->tests_);
  mtx_destroy (&self->output_lock_);
}

size_t
TestManager_get_total_count (const TestManager *self)
{
  return Vec_TestFnWrapper_len (&self->tests_);
}

size_t
TestManager_get_failed_count (const TestManager *self)
{
  size_t count = 0;
  for (const TestFnWrapper *test = Vec_TestFnWrapper_cbegin (&self->tests_);
       test < Vec_TestFnWrapper_cend (&self->tests_); test++)
    {
      if (test->status_ == TEST_FAILED)
        {
          count++;
        }
    }
  return count;
}

void
TestManager_push (TestManager *self, TestFnWrapper test)
{
  test.output_lock_ = &self->output_lock_;
  Vec_TestFnWrapper_push (&self->tests_, test);
}

void
TestManager_push_tests (TestManager *self, Array_TestFnWrapper tests)
{
  for (const TestFnWrapper *test = Array_TestFnWrapper_cbegin (&tests);
       test < Array_TestFnWrapper_cend (&tests); test++)
    {
      TestManager_push (self, *test);
    }
}

static void
test_job (void *test_fn_wrapper)
{
  TestFnWrapper *test = test_fn_wrapper;
  String_push_cstring (&test->message_, "test ");
  String_push_cstring (&test->message_, test->file_);
  String_push_cstring (&test->message_, ":");
  String_push_cstring (&test->message_, test->func_);
  String_push_cstring (&test->message_, " ... ");
  test->test_fn_ (test);
  if (test->status_ == TEST_NOT_RUN)
    {
      test->status_ = TEST_OK;
      String_push_cstring (&test->message_, "\e[32mok\e[0m\n");
    }
  TestFnWrapper_lock (test);
  printf ("%.*s", (int)String_len (&test->message_),
          String_cbegin (&test->message_));
  TestFnWrapper_unlock (test);
}

void
TestManager_run (TestManager *self)
{
  ThreadPool pool = ThreadPool_new (self->num_threads_);
  for (TestFnWrapper *test = Vec_TestFnWrapper_begin (&self->tests_);
       test < Vec_TestFnWrapper_end (&self->tests_); test++)
    {
      ThreadPool_execute (&pool, test_job, test);
    }
  ThreadPool_drop (&pool);
}
