/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "big_int.h"

#include <assert.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "option_macro.h"
#include "vec.h"

#define U32_BITS (32)

NEO_IMPL_OPTION (BigInt, BigInt)

BigInt
BigInt_new ()
{
  return (BigInt){ .sign_ = BIG_INT_ZERO, .digits_ = Vec_u32_new () };
}

void
BigInt_drop (BigInt *self)
{
  Vec_u32_drop (&self->digits_);
}

uint64_t
carrying_mul (uint32_t left, uint32_t right, uint32_t carry)
{
  return (uint64_t)left * (uint64_t)right + (uint64_t)carry;
}

static void
digits_carrying_mul_u32_in_place (Vec_u32 *left, uint32_t right,
                                  uint32_t carry)
{
  if (right == 0)
    {
      Vec_u32_clear (left);
      if (carry)
        {
          Vec_u32_push (left, carry);
        }
    }
  else if (Vec_u32_is_empty (left))
    {
      if (carry)
        {
          Vec_u32_push (left, carry);
        }
    }
  else /* Both left and right are not zero.  */
    {
      for (uint32_t *left_ptr = Vec_u32_begin (left);
           left_ptr < Vec_u32_end (left); left_ptr++)
        {
          uint64_t prod = carrying_mul (*left_ptr, right, carry);
          *left_ptr = prod;
          carry = prod >> U32_BITS;
        }
      if (carry)
        {
          Vec_u32_push (left, carry);
        }
    }
}

/* Returns -1 if the char is invalid, otherwise returns 0-36.  */
static int
from_char_radix (char c, size_t radix)
{
  assert (radix >= 2 && radix <= 36);
  int n = -1;
  if (c >= '0' && c <= '9')
    {
      n = c - '0';
    }
  else if (c >= 'a' && c <= 'z')
    {
      n = c - 'a' + 10;
    }
  else if (c >= 'A' && c <= 'Z')
    {
      n = c - 'A' + 10;
    }
  else
    {
      return -1;
    }
  return n <= (int)radix - 1 ? n : -1;
}

static Option_BigInt
BigInt_from_str_radix (const char *src, size_t len, size_t radix)
{
  if (len == 0 || radix < 2 || radix > 36)
    {
      return Option_BigInt_none ();
    }
  const char *src_cend = src + len;
  enum BigIntSign sign = BIG_INT_POSITIVE;
  if (*src == '+')
    {
      sign = BIG_INT_POSITIVE;
      src++;
    }
  else if (*src == '-')
    {
      sign = BIG_INT_NEGATIVE;
      src++;
    }
  Vec_u32 digits = Vec_u32_new ();
  for (; src < src_cend; src++)
    {
      int n = from_char_radix (*src, radix);
      if (n < 0)
        {
          Vec_u32_drop (&digits);
          return Option_BigInt_none ();
        }
      digits_carrying_mul_u32_in_place (&digits, radix, n);
    }
  return Option_BigInt_some (
      (BigInt){ .sign_ = Vec_u32_is_empty (&digits) ? BIG_INT_ZERO : sign,
                .digits_ = digits });
}

Option_BigInt
BigInt_from_str (const char *src, size_t len)
{
  return BigInt_from_str_radix (src, len, 10);
}

bool
BigInt_is_zero (const BigInt *self)
{
  return self->sign_ == BIG_INT_ZERO;
}

static int
digits_cmp (const Vec_u32 *left, const Vec_u32 *right)
{
  const size_t left_len = Vec_u32_len (left);
  const size_t right_len = Vec_u32_len (right);
  if (left_len > right_len)
    {
      return 1;
    }
  else if (left_len < right_len)
    {
      return -1;
    }
  else
    {
      size_t i = left_len;
      while (i-- > 0)
        {
          size_t left_digit = Vec_u32_cbegin (left)[i];
          size_t right_digit = Vec_u32_cbegin (right)[i];
          if (left_digit > right_digit)
            {
              return 1;
            }
          else if (left_digit < right_digit)
            {
              return -1;
            }
        }
      return 0;
    }
}

int
BigInt_cmp (const BigInt *left, const BigInt *right)
{
  if (left->sign_ == right->sign_)
    {
      switch (left->sign_)
        {
        case BIG_INT_NEGATIVE:
          return digits_cmp (&right->digits_, &left->digits_);
        case BIG_INT_ZERO:
          return 0;
        case BIG_INT_POSITIVE:
          return digits_cmp (&left->digits_, &right->digits_);
        }
    }
  if (left->sign_ == BIG_INT_NEGATIVE)
    {
      return -1;
    }
  else if (left->sign_ == BIG_INT_POSITIVE)
    {
      return 1;
    }
  else
    {
      return right->sign_ == BIG_INT_NEGATIVE ? 1 : -1;
    }
}

static void
raw_digits_add_u32_in_place (uint32_t *acc, uint32_t right)
{
  uint64_t sum = (uint64_t)*acc + (uint64_t)right;
  *acc = sum;
  acc++;
  bool carry = sum >> U32_BITS;
  while (carry)
    {
      sum = (uint64_t)*acc + (uint64_t)carry;
      *acc = sum;
      acc++;
      carry = sum >> U32_BITS;
    }
}

/* Multiply accumulate operation: acc += left * right */
static void
raw_digits_mac_u32 (uint32_t *acc, const uint32_t *left, size_t left_len,
                    uint32_t right)
{
  if (right == 0)
    {
      return;
    }
  uint32_t carry = 0;
  const uint32_t *left_cend = left + left_len;
  for (const uint32_t *left_digit = left; left_digit < left_cend; left_digit++)
    {
      uint64_t prod = carrying_mul (*left_digit, right, carry);
      raw_digits_add_u32_in_place (acc, prod);
      acc++;
      carry = prod >> U32_BITS;
    }
  raw_digits_add_u32_in_place (acc, carry);
}

static Vec_u32
digits_mul (const Vec_u32 *left, const Vec_u32 *right)
{
  Vec_u32 prod = Vec_u32_new ();
  /* For B-based numbers left and right, suppose left has m digits and right
   * has n digits. The max of left * right is
   * (B^m - 1)(B^n - 1) = B^{m+n} - (B^m + B^n) + 1 <= B^{m+n} - 1
   * Therefore, m+n digits are enough for the product.  */
  Vec_u32_resize (&prod, Vec_u32_len (left) + Vec_u32_len (right), 0);
  const uint32_t *left_cbegin = Vec_u32_cbegin (left);
  size_t left_len = Vec_u32_len (left);
  for (size_t idx = 0; idx < Vec_u32_len (right); idx++)
    {
      raw_digits_mac_u32 (Vec_u32_begin (&prod) + idx, left_cbegin, left_len,
                          Vec_u32_cbegin (right)[idx]);
    }
  /* Remove the trailing zeros.  */
  const uint32_t *back = Vec_u32_cend (&prod);
  while (back-- > Vec_u32_cbegin (&prod))
    {
      if (*back)
        {
          break;
        }
    }
  Vec_u32_resize (&prod, back + 1 - Vec_u32_cbegin (&prod), 0);
  return prod;
}

BigInt
BigInt_mul (const BigInt *left, const BigInt *right)
{
  if (BigInt_is_zero (left) || BigInt_is_zero (right))
    {
      return BigInt_new ();
    }
  return (BigInt){ .sign_ = left->sign_ == right->sign_ ? BIG_INT_POSITIVE
                                                        : BIG_INT_NEGATIVE,
                   .digits_ = digits_mul (&left->digits_, &right->digits_) };
}

#ifdef TESTS
#include "test.h"

#include <string.h>

static int
cmp_binary_op (BigInt (*op) (const BigInt *, const BigInt *), const char *left,
               const char *right, const char *expect)
{
  Option_BigInt left_opt = BigInt_from_str (left, strlen (left));
  Option_BigInt right_opt = BigInt_from_str (right, strlen (right));
  Option_BigInt expect_opt = BigInt_from_str (expect, strlen (expect));
  BigInt ileft = Option_BigInt_unwrap (&left_opt);
  BigInt iright = Option_BigInt_unwrap (&right_opt);
  BigInt iexpect = Option_BigInt_unwrap (&expect_opt);
  BigInt iresult = op (&ileft, &iright);
  int cmp = BigInt_cmp (&iresult, &iexpect);
  BigInt_drop (&ileft);
  BigInt_drop (&iright);
  BigInt_drop (&iexpect);
  BigInt_drop (&iresult);
  return cmp;
}

NEO_TEST (test_raw_digits_add_u32_in_place_00)
{
  uint32_t acc[] = { UINT32_MAX, 1 };
  raw_digits_add_u32_in_place (acc, 1);
  ASSERT_U64_EQ (acc[0], 0);
  ASSERT_U64_EQ (acc[1], 2);
  raw_digits_add_u32_in_place (acc, UINT32_MAX);
  raw_digits_add_u32_in_place (acc, UINT32_MAX);
  ASSERT_U64_EQ (acc[0], UINT32_MAX - 1);
  ASSERT_U64_EQ (acc[1], 3);
}

NEO_TEST (test_raw_digits_mac_00)
{
  uint32_t acc[] = { 0, 0, 0, 0 };
  uint32_t left[] = { UINT32_MAX, UINT32_MAX };
  raw_digits_mac_u32 (acc, left, sizeof (left) / sizeof (uint32_t),
                      UINT32_MAX);
  ASSERT_U64_EQ (acc[0], 1);
  ASSERT_U64_EQ (acc[1], UINT32_MAX);
  ASSERT_U64_EQ (acc[2], UINT32_MAX - 1);
}

NEO_TEST (test_mul_00)
{
  ASSERT_I64_EQ (cmp_binary_op (BigInt_mul, "11", "22", "242"), 0);
  ASSERT_I64_EQ (cmp_binary_op (BigInt_mul, "-11", "22", "-242"), 0);
  ASSERT_I64_EQ (cmp_binary_op (BigInt_mul, "11", "-22", "-242"), 0);
  ASSERT_I64_EQ (cmp_binary_op (BigInt_mul, "4294967298", "12884901892",
                                "55340232264078327816"),
                 0);
  ASSERT_I64_EQ (cmp_binary_op (BigInt_mul, "36893488147419103232",
                                "73786976294838206464",
                                "2722258935367507707706996859454145691648"),
                 0);
  ASSERT_I64_EQ (cmp_binary_op (BigInt_mul, "-34359738377", "-38654705674",
                                "1328165573998577451098"),
                 0);
  ASSERT_I64_EQ (cmp_binary_op (BigInt_mul,
                                "-680564733841876926926749214863536423226",
                                "1361129467683753853853498429727072877239",
                                "-92633671389852956338856788006950328463349564"
                                "3820386881829485763935602646353014"),
                 0);
  ASSERT_I64_EQ (cmp_binary_op (BigInt_mul, "18446744073709551615",
                                "18446744073709551615",
                                "340282366920938463426481119284349108225"),
                 0);
}

NEO_TESTS (big_int_tests, test_raw_digits_add_u32_in_place_00,
           test_raw_digits_mac_00, test_mul_00)

#endif
