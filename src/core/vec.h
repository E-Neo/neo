/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_CORE_VEC_H_
#define NEO_CORE_VEC_H_

#include <stddef.h>
#include <threads.h>

#include "core/macro/vec.h"

NEO_DECL_VEC (char, char)
NEO_DECL_VEC (size_t, size_t)
NEO_DECL_VEC (thrd_t, thrd_t)

#endif
