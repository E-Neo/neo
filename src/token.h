/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_TOKEN_H
#define NEO_TOKEN_H

#include <stdbool.h>

#include "array_macro.h"
#include "span.h"
#include "vec_macro.h"

enum TokenKind
{
#define NEO_TOKEN_LIT(N, UNUSED) TOKEN_##N,
#include "token_lit.def"
#undef NEO_TOKEN_LIT
#define NEO_TOKEN(N) TOKEN_##N,
#include "token.def"
#undef NEO_TOKEN
};

NEO_DECL_ARRAY (TokenKind, enum TokenKind)
NEO_DECL_VEC (TokenKind, enum TokenKind)

typedef struct Token
{
  enum TokenKind kind_;
  Span span_;
} Token;

NEO_DECL_VEC (Token, Token)

bool Token_is_eof (const Token *self);

#endif
