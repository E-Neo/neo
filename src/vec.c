/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "vec.h"

#include "vec_macro.h"

#include <threads.h>

NEO_IMPL_VEC (char, char)
NEO_IMPL_VEC (const_char_ptr, const char *)
NEO_IMPL_VEC (Vec_char, Vec_char)
NEO_IMPL_VEC (thrd_t, thrd_t)
