/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "parser.h"

#include <stdbool.h>
#include <stddef.h>

#include "ast_node.h"
#include "diagnostic.h"
#include "lexer.h"
#include "span.h"
#include "string.h"
#include "token.h"
#include "vec.h"

#define INVALID_AST_NODE_ID (0)

Parser
Parser_new (const Vec_Token *tokens, DiagnosticManager *diag_mgr,
            ASTNodeManager *ast_mgr)
{
  ASTNodeManager_push_invalid (ast_mgr);
  return (Parser){ .tokens_ = tokens,
                   .diag_mgr_ = diag_mgr,
                   .ast_mgr_ = ast_mgr,
                   .cursor_ = Vec_Token_cbegin (tokens) };
}

static const char *
Parser_end_pos (const Parser *self)
{
  return String_cend (SourceFile_get_content (self->diag_mgr_->file_));
}

Span
Parser_end_span (const Parser *self)
{
  return Span_new (Parser_end_pos (self), 0);
}

static void
Parser_skip (Parser *self, size_t count)
{
  self->cursor_ += count;
}

static bool
Parser_expect (Parser *self, enum TokenKind kind)
{
  if (self->cursor_->kind_ == kind)
    {
      return true;
    }
  DiagnosticManager_diagnose_expected_token (self->diag_mgr_,
                                             self->cursor_->span_, kind);
  return false;
}

static bool
Parser_expect_and_skip (Parser *self, enum TokenKind kind)
{
  if (Parser_expect (self, kind))
    {
      Parser_skip (self, 1);
      return true;
    }
  return false;
}

static ASTNodeId Parser_parse_expr (Parser *self);

static ASTNodeId
Parser_parse_if_then_else (Parser *self)
{
  const char *span_begin = Span_cbegin (&self->cursor_->span_);
  if (!Parser_expect_and_skip (self, TOKEN_IF))
    {
      return INVALID_AST_NODE_ID;
    }
  ASTNodeId if_expr = Parser_parse_expr (self);
  if (!if_expr)
    {
      return INVALID_AST_NODE_ID;
    }
  if (!Parser_expect_and_skip (self, TOKEN_THEN))
    {
      return INVALID_AST_NODE_ID;
    }
  ASTNodeId then_expr = Parser_parse_expr (self);
  if (!then_expr)
    {
      return INVALID_AST_NODE_ID;
    }
  if (!Parser_expect_and_skip (self, TOKEN_ELSE))
    {
      return INVALID_AST_NODE_ID;
    }
  ASTNodeId else_expr = Parser_parse_expr (self);
  if (!else_expr)
    {
      return INVALID_AST_NODE_ID;
    }
  return ASTNodeManager_push_if_then_else (
      self->ast_mgr_,
      Span_new (
          span_begin,
          Span_cend (
              &ASTNodeManager_get_node (self->ast_mgr_, else_expr)->span_)
              - span_begin),
      if_expr, then_expr, else_expr);
}

static ASTNodeId
Parser_parse_var (Parser *self)
{
  if (!Parser_expect (self, TOKEN_NAME))
    {
      return INVALID_AST_NODE_ID;
    }
  ASTNodeId var
      = ASTNodeManager_push_var (self->ast_mgr_, self->cursor_->span_);
  Parser_skip (self, 1);
  return var;
}

static ASTNodeId
Parser_parse_type (Parser *self)
{
  if (!Parser_expect (self, TOKEN_NAME))
    {
      return INVALID_AST_NODE_ID;
    }
  ASTNodeId var
      = ASTNodeManager_push_type (self->ast_mgr_, self->cursor_->span_);
  Parser_skip (self, 1);
  return var;
}

static ASTNodeId
Parser_parse_let (Parser *self)
{
  const char *span_begin = Span_cbegin (&self->cursor_->span_);
  if (!Parser_expect_and_skip (self, TOKEN_LET))
    {
      return INVALID_AST_NODE_ID;
    }
  ASTNodeId var = Parser_parse_var (self);
  if (!var)
    {
      return INVALID_AST_NODE_ID;
    }
  if (!Parser_expect_and_skip (self, TOKEN_COLON))
    {
      return INVALID_AST_NODE_ID;
    }
  ASTNodeId type = Parser_parse_type (self);
  if (!type)
    {
      return INVALID_AST_NODE_ID;
    }
  if (!Parser_expect_and_skip (self, TOKEN_EQ))
    {
      return INVALID_AST_NODE_ID;
    }
  ASTNodeId expr = Parser_parse_expr (self);
  if (!expr)
    {
      return INVALID_AST_NODE_ID;
    }
  if (!Parser_expect_and_skip (self, TOKEN_IN))
    {
      return INVALID_AST_NODE_ID;
    }
  ASTNodeId body = Parser_parse_expr (self);
  if (!body)
    {
      return INVALID_AST_NODE_ID;
    }
  return ASTNodeManager_push_let (
      self->ast_mgr_,
      Span_new (span_begin,
                Span_cend (ASTNodeManager_get_span (self->ast_mgr_, body))
                    - span_begin),
      var, type, expr, body);
}

static ASTNodeId
Parser_parse_expr (Parser *self)
{
  switch (self->cursor_->kind_)
    {
    case TOKEN_INVALID:
      {
        DiagnosticManager_diagnose_invalid_token (self->diag_mgr_,
                                                  self->cursor_->span_);
        return INVALID_AST_NODE_ID;
      }
    case TOKEN_FALSE:
      {
        ASTNodeId id = ASTNodeManager_push_lit (
            self->ast_mgr_, self->cursor_->span_, AST_LIT_FALSE);
        self->cursor_++;
        return id;
      }
    case TOKEN_TRUE:
      {
        ASTNodeId id = ASTNodeManager_push_lit (
            self->ast_mgr_, self->cursor_->span_, AST_LIT_TRUE);
        self->cursor_++;
        return id;
      }
    case TOKEN_IF:
      {
        return Parser_parse_if_then_else (self);
      }
    case TOKEN_LET:
      {
        return Parser_parse_let (self);
      }
    case TOKEN_NAME:
      {
        return Parser_parse_var (self);
      }
    default:
      {
        DiagnosticManager_diagnose_unexpected_token (self->diag_mgr_,
                                                     self->cursor_->span_);
        return INVALID_AST_NODE_ID;
      }
    }
}

ASTNodeId
Parser_parse (Parser *self)
{
  ASTNodeId id = Parser_parse_expr (self);
  if (id && !Token_is_eof (self->cursor_))
    {
      DiagnosticManager_diagnose_unexpected_token (self->diag_mgr_,
                                                   self->cursor_->span_);
      return INVALID_AST_NODE_ID;
    }
  return id;
}

#ifdef TESTS
#include "test.h"

typedef struct ParserTest
{
  SourceFile file_;
  Vec_Token tokens_;
  ASTNodeManager ast_mgr_;
  DiagnosticManager diag_mgr_;
  Parser parser_;
} ParserTest;

static void
ParserTest_init (ParserTest *self, const char *content)
{
  self->file_ = SourceFile_new (String_from_cstring ("test"),
                                String_from_cstring (content));
  Span content_span = Span_from_string (SourceFile_get_content (&self->file_));
  Lexer lexer = Lexer_new (&content_span);
  self->tokens_ = Vec_Token_new ();
  Token token;
  do
    {
      token = Lexer_next (&lexer);
      Vec_Token_push (&self->tokens_, token);
    }
  while (!Token_is_eof (&token));
  self->ast_mgr_ = ASTNodeManager_new ();
  self->diag_mgr_ = DiagnosticManager_new (&self->file_);
  self->parser_
      = Parser_new (&self->tokens_, &self->diag_mgr_, &self->ast_mgr_);
}

static void
ParserTest_drop (ParserTest *self)
{
  SourceFile_drop (&self->file_);
  Vec_Token_drop (&self->tokens_);
  ASTNodeManager_drop (&self->ast_mgr_);
  DiagnosticManager_drop (&self->diag_mgr_);
}

static Parser *
ParserTest_borrow_parser (ParserTest *self)
{
  return &self->parser_;
}

static const DiagnosticManager *
ParserTest_get_diagnostic_manager (const ParserTest *self)
{
  return &self->diag_mgr_;
}

NEO_TEST (test_parse_true_00)
{
  ParserTest parser_test;
  ParserTest_init (&parser_test, "true");
  Parser_parse (ParserTest_borrow_parser (&parser_test));
  ASSERT_U64_EQ (DiagnosticManager_num_total (
                     ParserTest_get_diagnostic_manager (&parser_test)),
                 0);
  ParserTest_drop (&parser_test);
}

NEO_TEST (test_parse_true_01)
{
  ParserTest parser_test;
  ParserTest_init (&parser_test, "true\n");
  Parser_parse (ParserTest_borrow_parser (&parser_test));
  ASSERT_U64_EQ (DiagnosticManager_num_total (
                     ParserTest_get_diagnostic_manager (&parser_test)),
                 0);
  ParserTest_drop (&parser_test);
}

NEO_TEST (test_parse_if_00)
{
  ParserTest parser_test;
  ParserTest_init (&parser_test, "if true then\n"
                                 "    true\n"
                                 "else\n"
                                 "    false\n");
  Parser_parse (ParserTest_borrow_parser (&parser_test));
  ASSERT_U64_EQ (DiagnosticManager_num_total (
                     ParserTest_get_diagnostic_manager (&parser_test)),
                 0);
  ParserTest_drop (&parser_test);
}

NEO_TEST (test_parse_let_identifier_00)
{
  ParserTest parser_test;
  ParserTest_init (&parser_test, "let x: Bool = true in x");
  Parser_parse (ParserTest_borrow_parser (&parser_test));
  ASSERT_U64_EQ (DiagnosticManager_num_total (
                     ParserTest_get_diagnostic_manager (&parser_test)),
                 0);
  ParserTest_drop (&parser_test);
}

NEO_TEST (test_parse_let_identifier_01)
{
  ParserTest parser_test;
  ParserTest_init (&parser_test,
                   "let x: Bool = if true then true else false in x");
  Parser_parse (ParserTest_borrow_parser (&parser_test));
  ASSERT_U64_EQ (DiagnosticManager_num_total (
                     ParserTest_get_diagnostic_manager (&parser_test)),
                 0);
  ParserTest_drop (&parser_test);
}

NEO_TESTS (parser_tests, test_parse_true_00, test_parse_true_01,
           test_parse_if_00, test_parse_let_identifier_00,
           test_parse_let_identifier_01)
#endif
