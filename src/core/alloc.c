/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>

static inline void
handle_alloc_error ()
{
  fputs ("fatal error: out of memory", stderr);
  abort ();
}

void *
neo_malloc (size_t size)
{
  assert (size > 0);
  void *ptr = malloc (size);
  if (ptr == NULL)
    {
      handle_alloc_error ();
    }
  return ptr;
}

void *
neo_realloc (void *ptr, size_t size)
{
  assert (size > 0);
  ptr = realloc (ptr, size);
  if (ptr == NULL)
    {
      handle_alloc_error ();
    }
  return ptr;
}

void
neo_free (void *ptr)
{
  free (ptr);
}
