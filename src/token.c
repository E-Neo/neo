/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "token.h"

#include "array_macro.h"
#include "vec_macro.h"

NEO_IMPL_ARRAY (TokenKind, enum TokenKind)
NEO_IMPL_VEC (TokenKind, enum TokenKind)

NEO_IMPL_VEC (Token, Token)

bool
Token_is_eof (const Token *self)
{
  return self->kind_ == TOKEN_EOF;
}
