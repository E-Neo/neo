/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "result.h"

#include "result_macro.h"

#define NEO_RESULT(NT, NE, T, E) NEO_IMPL_RESULT (NT, NE, T, E)
#include "result.def"
#undef NEO_RESULT

#undef NEO_RESULT
