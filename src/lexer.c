/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "lexer.h"

#include <assert.h>
#include <stdbool.h>
#include <stddef.h>
#include <string.h>

#include "span.h"
#include "token.h"

static bool
is_newline (char c)
{
  return c == '\n';
}

static bool
is_whitespace (char c)
{
  return c == ' ' || c == '\n';
}

static bool
is_underscore (char c)
{
  return c == '_';
}

static bool
is_digit (char c)
{
  return c >= '0' && c <= '9';
}

static bool
is_lowercase (char c)
{
  return c >= 'a' && c <= 'z';
}

static bool
is_uppercase (char c)
{
  return c >= 'A' && c <= 'Z';
}

static bool
is_letter (char c)
{
  return is_lowercase (c) || is_uppercase (c);
}

static bool
is_terminal (char c)
{
  return !(is_digit (c) || is_letter (c) || is_underscore (c));
}

static size_t
Lexer_seeing_token_lit (const Lexer *self, enum TokenKind kind)
{
  switch (kind)
    {
#define NEO_TOKEN_LIT(T, L)                                                   \
  case TOKEN_##T:                                                             \
    {                                                                         \
      const char token[] = #L;                                                \
      size_t token_len = sizeof (token) - 1;                                  \
      assert (token_len > 0);                                                 \
      const char *token_end = self->cursor_ + token_len;                      \
      if (token_end > Span_cend (&self->span_)                                \
          || memcmp (self->cursor_, token, token_len))                        \
        {                                                                     \
          return 0;                                                           \
        }                                                                     \
      if (is_terminal (*(token_end - 1))                                      \
          || token_end == Span_cend (&self->span_)                            \
          || is_terminal (*token_end))                                        \
        {                                                                     \
          return token_len;                                                   \
        }                                                                     \
      return 0;                                                               \
    }
#include "token_lit.def"
#undef NEO_TOKEN_LIT
    default:
      return 0;
    }
}

static size_t
Lexer_seeing_name (const Lexer *self)
{
  const char *cursor = self->cursor_;
  if (cursor >= Span_cend (&self->span_)
      || !(is_underscore (*cursor) || is_letter (*cursor)))
    {
      return 0;
    }
  cursor++;
  while (cursor < Span_cend (&self->span_)
         && (is_digit (*cursor) || is_letter (*cursor)
             || is_underscore (*cursor)))
    {
      cursor++;
    }
  return cursor - self->cursor_;
}

static void
Lexer_skip (Lexer *self, size_t count)
{
  self->cursor_ += count;
}

static void
Lexer_skip_whitespace_or_comments (Lexer *self)
{
  while (self->cursor_ < Span_cend (&self->span_))
    {
      if (is_whitespace (*self->cursor_))
        {
          Lexer_skip (self, 1);
        }
      else if (*self->cursor_ == '#')
        {
          Lexer_skip (self, 1);
          while (!is_newline (*self->cursor_))
            {
              Lexer_skip (self, 1);
            }
        }
      else
        {
          break;
        }
    }
}

static size_t
Lexer_next_whitespace_or_comments (Lexer *self)
{
  const char *cursor = self->cursor_;
  while (cursor < Span_cend (&self->span_))
    {
      if (is_whitespace (*cursor) || *cursor == '#')
        {
          break;
        }
      cursor++;
    }
  return cursor - self->cursor_;
}

Lexer
Lexer_new (const Span *span)
{
  return (Lexer){ .span_ = *span, .cursor_ = Span_cbegin (span) };
}

Option_Token
Lexer_next (Lexer *self)
{
  Lexer_skip_whitespace_or_comments (self);
  size_t skip_count = 0;
  const char *token_begin = NULL;
#define NEO_TOKEN_LIT(T, UNUSED)                                              \
  if ((skip_count = Lexer_seeing_token_lit (self, TOKEN_##T)) != 0)           \
    {                                                                         \
      token_begin = self->cursor_;                                            \
      Lexer_skip (self, skip_count);                                          \
      return Option_Token_some ((Token){                                      \
          .kind_ = TOKEN_##T, .span_ = Span_new (token_begin, skip_count) }); \
    }
#include "token_lit.def"
#undef NEO_TOKEN_LIT
  if ((skip_count = Lexer_seeing_name (self)) != 0)
    {
      token_begin = self->cursor_;
      Lexer_skip (self, skip_count);
      return Option_Token_some ((Token){
          .kind_ = TOKEN_NAME, .span_ = Span_new (token_begin, skip_count) });
    }
  if (self->cursor_ < Span_cend (&self->span_))
    {
      token_begin = self->cursor_;
      skip_count = Lexer_next_whitespace_or_comments (self);
      Lexer_skip (self, skip_count);
      return Option_Token_some (
          (Token){ .kind_ = TOKEN_INVALID,
                   .span_ = Span_new (token_begin, skip_count) });
    }
  return Option_Token_none ();
}

#ifdef TESTS
#include "test.h"

static Vec_Token
lex_tokens (const char *content)
{
  Span span = Span_new (content, strlen (content));
  Lexer lexer = Lexer_new (&span);
  Vec_Token tokens = Vec_Token_new ();
  Option_Token opt_token = Lexer_next (&lexer);
  while (Option_Token_is_some (&opt_token))
    {
      Vec_Token_push (&tokens, Option_Token_unwrap (&opt_token));
      opt_token = Lexer_next (&lexer);
    }
  return tokens;
}

NEO_TEST (test_seeing_token_lit_00)
{
  Span span = Span_from_cstring ("true");
  Lexer lexer = Lexer_new (&span);
  ASSERT_U64_EQ (Lexer_seeing_token_lit (&lexer, TOKEN_TRUE), 4);
}

NEO_TEST (test_seeing_token_lit_01)
{
  Span span = Span_from_cstring ("{true}");
  Lexer lexer = Lexer_new (&span);
  ASSERT_U64_EQ (Lexer_seeing_token_lit (&lexer, TOKEN_LBRACE), 1);
}

NEO_TEST (test_lex_true_00)
{
  Vec_Token tokens = lex_tokens ("true");
  ASSERT_U64_EQ (Vec_Token_len (&tokens), 1);
  ASSERT_U64_EQ (Vec_Token_cbegin (&tokens)->kind_, TOKEN_TRUE);
  Vec_Token_drop (&tokens);
}

NEO_TEST (test_lex_true_01)
{
  Vec_Token tokens = lex_tokens ("true false truetrue");
  ASSERT_U64_EQ (Vec_Token_len (&tokens), 3);
  ASSERT_U64_EQ (Vec_Token_cbegin (&tokens)[0].kind_, TOKEN_TRUE);
  ASSERT_U64_EQ (Vec_Token_cbegin (&tokens)[1].kind_, TOKEN_FALSE);
  ASSERT_U64_EQ (Vec_Token_cbegin (&tokens)[2].kind_, TOKEN_NAME);
  Vec_Token_drop (&tokens);
}

NEO_TESTS (lexer_tests, test_seeing_token_lit_00, test_seeing_token_lit_01,
           test_lex_true_00, test_lex_true_01)
#endif