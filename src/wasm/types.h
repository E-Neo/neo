/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_WASM_TYPES_H_
#define NEO_WASM_TYPES_H_

#include <stdint.h>

typedef struct WasmModule
{
  uint32_t magic_;
  uint32_t version_;
} WasmModule;

#endif
