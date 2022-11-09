/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_CORE_ALLOC_H_
#define NEO_CORE_ALLOC_H_

#include <stddef.h>

void *neo_malloc (size_t size);
void *neo_realloc (void *ptr, size_t size);
void *neo_free (void *ptr);

#endif
