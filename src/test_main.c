/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "test.h"

#include <stdio.h>
#include <time.h>

static double
timespec_to_secs (const struct timespec *t)
{
  return (double)t->tv_sec + (double)t->tv_nsec / 1000000000.0;
}

int
main ()
{
  struct timespec time_begin, time_end;
  timespec_get (&time_begin, TIME_UTC);
  TestManager test_mgr = TestManager_new (8);
#define NEO_PUSH_TESTS(NAME) TestManager_push_tests (&test_mgr, NAME ());
#include "tests.def"
#undef NEO_PUSH_TESTS
  TestManager_run (&test_mgr);
  printf ("\ntest result: %s. %zu passed; %zu failed; ",
          TestManager_get_failed_count (&test_mgr) ? "\e[31mFAILED\e[0m"
                                                   : "\e[32mok\e[0m",
          TestManager_get_total_count (&test_mgr)
              - TestManager_get_failed_count (&test_mgr),
          TestManager_get_failed_count (&test_mgr));
  TestManager_drop (&test_mgr);
  timespec_get (&time_end, TIME_UTC);
  printf ("finished in %.2lfs\n",
          timespec_to_secs (&time_end) - timespec_to_secs (&time_begin));
  return 0;
}
