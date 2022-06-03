/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_VEC_H
#define NEO_VEC_H

#include "vec_macro.h"

#include <stddef.h>
#include <stdint.h>
#include <threads.h>

NEO_DECL_VEC (char, char)
NEO_DECL_VEC (const_char_ptr, const char *)
NEO_DECL_VEC (Vec_char, Vec_char)
NEO_DECL_VEC (u32, uint32_t)
NEO_DECL_VEC (thrd_t, thrd_t)

#endif
