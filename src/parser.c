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

static bool
Parser_seeing (const Parser *self, enum TokenKind kind)
{
  if (self->cursor_->kind_ == kind)
    {
      return true;
    }
  return false;
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
      Parser_skip (self, 1);
      return true;
    }
  DiagnosticManager_diagnose_expected_token (self->diag_mgr_,
                                             self->cursor_->span_, kind);
  return false;
}

static ASTNodeId Parser_parse_expr (Parser *self);

static ASTNodeId
Parser_parse_void (Parser *self)
{
  if (Vec_Token_is_empty (self->tokens_))
    {
      DiagnosticManager_diagnose_expected_node (
          self->diag_mgr_, Parser_end_span (self), AST_VOID);
      return INVALID_AST_NODE_ID;
    }
  return ASTNodeManager_push_void (
      self->ast_mgr_, Span_new (Span_cend (&(self->cursor_ - 1)->span_), 0));
}

static ASTNodeId
Parser_parse_block (Parser *self)
{
  const char *span_begin = Span_cbegin (&self->cursor_->span_);
  if (!Parser_expect (self, TOKEN_LBRACE))
    {
      return INVALID_AST_NODE_ID;
    }
  Vec_ASTNodeId exprs = Vec_ASTNodeId_new ();
  while (!Parser_seeing (self, TOKEN_RBRACE))
    {
      if (Vec_ASTNodeId_is_empty (&exprs)
          || (self->cursor_ - 1)->kind_ == TOKEN_RBRACE)
        {
          if (Parser_seeing (self, TOKEN_SEMICOLON))
            {
              DiagnosticManager_diagnose_unexpected_token (
                  self->diag_mgr_, self->cursor_->span_);
              Vec_ASTNodeId_drop (&exprs);
              return INVALID_AST_NODE_ID;
            }
        }
      else
        {
          if (!Parser_expect (self, TOKEN_SEMICOLON))
            {
              Vec_ASTNodeId_drop (&exprs);
              return INVALID_AST_NODE_ID;
            }
        }
      if (Parser_seeing (self, TOKEN_RBRACE))
        {
          ASTNodeId expr = Parser_parse_void (self);
          Vec_ASTNodeId_push (&exprs, expr);
          break;
        }
      ASTNodeId expr = Parser_parse_expr (self);
      if (!expr)
        {
          Vec_ASTNodeId_drop (&exprs);
          return INVALID_AST_NODE_ID;
        }
      Vec_ASTNodeId_push (&exprs, expr);
    }
  const char *span_end = Span_cend (&self->cursor_->span_);
  Parser_skip (self, 1);
  return ASTNodeManager_push_block (
      self->ast_mgr_, Span_new (span_begin, span_end - span_begin), exprs);
}

static ASTNodeId
Parser_parse_if_then_else_handle_else_block (Parser *self,
                                             const char *span_begin,
                                             ASTNodeId if_expr,
                                             ASTNodeId then_block,
                                             ASTNodeId else_block)
{
  if (!else_block)
    {
      return INVALID_AST_NODE_ID;
    }
  const char *span_end
      = Span_cend (ASTNodeManager_get_span (self->ast_mgr_, else_block));
  return ASTNodeManager_push_if_then_else (
      self->ast_mgr_, Span_new (span_begin, span_end - span_begin), if_expr,
      then_block, else_block);
}

static ASTNodeId
Parser_parse_if_then_else_without_first_token (Parser *self,
                                               const char *span_begin)
{
  ASTNodeId if_expr = Parser_parse_expr (self);
  if (!if_expr)
    {
      return INVALID_AST_NODE_ID;
    }
  ASTNodeId then_block = Parser_parse_block (self);
  if (!then_block)
    {
      return INVALID_AST_NODE_ID;
    }
  if (Parser_seeing (self, TOKEN_ELIF))
    {
      const char *elif_begin = Span_cbegin (&self->cursor_->span_);
      if (!Parser_expect (self, TOKEN_ELIF))
        {
          return INVALID_AST_NODE_ID;
        }
      ASTNodeId elif
          = Parser_parse_if_then_else_without_first_token (self, elif_begin);
      if (!elif)
        {
          return INVALID_AST_NODE_ID;
        }
      Vec_ASTNodeId exprs = Vec_ASTNodeId_with_capacity (1);
      Vec_ASTNodeId_push (&exprs, elif);
      return Parser_parse_if_then_else_handle_else_block (
          self, span_begin, if_expr, then_block,
          ASTNodeManager_push_block (
              self->ast_mgr_, *ASTNodeManager_get_span (self->ast_mgr_, elif),
              exprs));
    }
  else if (!Parser_seeing (self, TOKEN_ELSE))
    {
      return Parser_parse_if_then_else_handle_else_block (
          self, span_begin, if_expr, then_block,
          ASTNodeManager_push_block (
              self->ast_mgr_,
              Span_new (Span_cend (ASTNodeManager_get_span (self->ast_mgr_,
                                                            then_block)),
                        0),
              Vec_ASTNodeId_new ()));
    }
  if (!Parser_expect (self, TOKEN_ELSE))
    {
      return INVALID_AST_NODE_ID;
    }
  return Parser_parse_if_then_else_handle_else_block (
      self, span_begin, if_expr, then_block, Parser_parse_block (self));
}

static ASTNodeId
Parser_parse_if_then_else (Parser *self)
{
  const char *span_begin = Span_cbegin (&self->cursor_->span_);
  if (!Parser_expect (self, TOKEN_IF))
    {
      return INVALID_AST_NODE_ID;
    }
  return Parser_parse_if_then_else_without_first_token (self, span_begin);
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
    case TOKEN_LBRACE:
      {
        return Parser_parse_block (self);
      }
    case TOKEN_IF:
      {
        return Parser_parse_if_then_else (self);
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
  ParserTest_init (&parser_test, "if true {\n"
                                 "    true\n"
                                 "} elif false {\n"
                                 "    false"
                                 "} else {\n"
                                 "    true"
                                 "}\n");
  Parser_parse (ParserTest_borrow_parser (&parser_test));
  ASSERT_U64_EQ (DiagnosticManager_num_total (
                     ParserTest_get_diagnostic_manager (&parser_test)),
                 0);
  ParserTest_drop (&parser_test);
}

NEO_TEST (test_parse_if_01)
{
  ParserTest parser_test;
  ParserTest_init (&parser_test, "if true {\n"
                                 "    true\n"
                                 "} elif false {\n"
                                 "    false"
                                 "}\n");
  Parser_parse (ParserTest_borrow_parser (&parser_test));
  ASSERT_U64_EQ (DiagnosticManager_num_total (
                     ParserTest_get_diagnostic_manager (&parser_test)),
                 0);
  ParserTest_drop (&parser_test);
}

NEO_TEST (test_parse_if_02)
{
  ParserTest parser_test;
  ParserTest_init (
      &parser_test,
      "if true { if true {} elif false {true;} else {false;} } else {}");
  Parser_parse (ParserTest_borrow_parser (&parser_test));
  ASSERT_U64_EQ (DiagnosticManager_num_total (
                     ParserTest_get_diagnostic_manager (&parser_test)),
                 0);
  ParserTest_drop (&parser_test);
}

NEO_TESTS (parser_tests, test_parse_true_00, test_parse_true_01,
           test_parse_if_00, test_parse_if_01, test_parse_if_02)
#endif
