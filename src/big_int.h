/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_BIG_INT_H
#define NEO_BIG_INT_H

#include <stdbool.h>
#include <stddef.h>

#include "option_macro.h"
#include "vec.h"

enum BigIntSign
{
  BIG_INT_ZERO,
  BIG_INT_NEGATIVE,
  BIG_INT_POSITIVE
};

typedef struct BigInt
{
  enum BigIntSign sign_;
  Vec_u32 digits_;
} BigInt;

NEO_DECL_OPTION (BigInt, BigInt)

/* Returns a zero.  */
BigInt BigInt_new ();
void BigInt_drop (BigInt *self);
Option_BigInt BigInt_from_str (const char *src, size_t len);
bool BigInt_is_zero (const BigInt *self);
/* Returns 0 if `left == right`,
 * returns 1 if `left > right`,
 * returns -1 if `left < right`.  */
int BigInt_cmp (const BigInt *left, const BigInt *right);
BigInt BigInt_mul (const BigInt *left, const BigInt *right);

#ifdef TESTS
#include "test.h"
Tests big_int_tests ();
#endif

#endif
