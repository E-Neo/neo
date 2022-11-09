/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "vec.h"

#include <stddef.h>
#include <threads.h>

#include "core/macro/vec.h"

NEO_IMPL_VEC (char, char)
NEO_IMPL_VEC (size_t, size_t)
NEO_IMPL_VEC (thrd_t, thrd_t)
