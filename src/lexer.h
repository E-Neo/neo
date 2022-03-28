/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_LEXER_H
#define NEO_LEXER_H

#include "span.h"
#include "token.h"

typedef struct Lexer
{
  Span span_;
  const char *cursor_;
} Lexer;

Lexer Lexer_new (const Span *span);
Option_Token Lexer_next (Lexer *self);

#ifdef TESTS
#include "test.h"
Tests lexer_tests ();
#endif

#endif
