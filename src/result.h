/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_RESULT_H
#define NEO_RESULT_H

#include "result_macro.h"

#define NEO_RESULT(NT, NE, T, E) NEO_DECL_RESULT (NT, NE, T, E)
#include "result.def"
#undef NEO_RESULT

#endif
