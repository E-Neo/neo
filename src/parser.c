/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "parser.h"

#include <assert.h>
#include <stdbool.h>
#include <stddef.h>

#include "ast_node.h"
#include "diagnostic.h"
#include "lexer.h"
#include "span.h"
#include "string.h"
#include "token.h"
#include "vec.h"

Parser
Parser_new (const Vec_Token *tokens, DiagnosticManager *diag_mgr,
            ASTNodeManager *ast_mgr)
{
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
Parser_seeing_after (Parser *self, size_t skip, enum TokenKind kind)
{
  if (self->cursor_ + skip >= Vec_Token_cend (self->tokens_))
    {
      return false;
    }
  return (self->cursor_ + skip)->kind_ == kind;
}

static bool
Parser_seeing (Parser *self, enum TokenKind kind)
{
  return Parser_seeing_after (self, 0, kind);
}

static bool
Parser_expect (Parser *self, enum TokenKind kind)
{
  if (Parser_seeing (self, kind))
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

static const char *
Parser_cursor_cbegin (const Parser *self)
{
  return Span_cbegin (&self->cursor_->span_);
}

static Span
Parser_span_from_last_node (const Parser *self, const char *cbegin,
                            ASTNodeId id)
{
  return Span_new (
      cbegin, Span_cend (&ASTNodeManager_get_node (self->ast_mgr_, id)->span_)
                  - cbegin);
}

static ASTNodeId Parser_parse_expr (Parser *self);

static ASTNodeId
Parser_parse_if_then_else (Parser *self)
{
  assert (Parser_seeing (self, TOKEN_IF));
  const char *cbegin = Parser_cursor_cbegin (self);
  Parser_skip (self, 1);
  ASTNodeId if_expr = Parser_parse_expr (self);
  if (is_invalid_ast_node_id (if_expr)
      || !Parser_expect_and_skip (self, TOKEN_THEN))
    {
      return get_invalid_ast_node_id ();
    }
  ASTNodeId then_expr = Parser_parse_expr (self);
  if (is_invalid_ast_node_id (then_expr)
      || !Parser_expect_and_skip (self, TOKEN_ELSE))
    {
      return get_invalid_ast_node_id ();
    }
  ASTNodeId else_expr = Parser_parse_expr (self);
  if (is_invalid_ast_node_id (else_expr))
    {
      return get_invalid_ast_node_id ();
    }
  return ASTNodeManager_push_if_then_else (
      self->ast_mgr_, Parser_span_from_last_node (self, cbegin, else_expr),
      if_expr, then_expr, else_expr);
}

static ASTNodeId
Parser_parse_var (Parser *self)
{
  if (!Parser_expect (self, TOKEN_NAME))
    {
      return get_invalid_ast_node_id ();
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
      return get_invalid_ast_node_id ();
    }
  ASTNodeId var
      = ASTNodeManager_push_type (self->ast_mgr_, self->cursor_->span_);
  Parser_skip (self, 1);
  return var;
}

static ASTNodeId
Parser_parse_lambda_single_param_with_type (Parser *self)
{
  assert (Parser_seeing (self, TOKEN_NAME));
  assert (Parser_seeing_after (self, 1, TOKEN_COLON));
  const char *cbegin = Parser_cursor_cbegin (self);
  ASTNodeId var = Parser_parse_var (self);
  assert (ASTNodeManager_get_node (self->ast_mgr_, var)->kind_ == AST_VAR);
  Parser_skip (self, 1); /* Skip the colon.  */
  ASTNodeId type = Parser_parse_type (self);
  if (is_invalid_ast_node_id (type)
      || !Parser_expect_and_skip (self, TOKEN_PLUS_GT))
    {
      return get_invalid_ast_node_id ();
    }
  ASTNodeId body = Parser_parse_expr (self);
  if (is_invalid_ast_node_id (body))
    {
      return get_invalid_ast_node_id ();
    }
  Vec_ASTNodeId vars = Vec_ASTNodeId_with_capacity (1);
  Vec_ASTNodeId_push (&vars, var);
  Vec_ASTNodeId types = Vec_ASTNodeId_with_capacity (1);
  Vec_ASTNodeId_push (&types, type);
  return ASTNodeManager_push_lambda (
      self->ast_mgr_, Parser_span_from_last_node (self, cbegin, body), vars,
      types, body);
}

static ASTNodeId
Parser_parse_lambda_single_param_without_type (Parser *self)
{
  assert (Parser_seeing (self, TOKEN_NAME));
  assert (Parser_seeing_after (self, 1, TOKEN_PLUS_GT));
  const char *cbegin = Parser_cursor_cbegin (self);
  ASTNodeId var = Parser_parse_var (self);
  assert (ASTNodeManager_get_node (self->ast_mgr_, var)->kind_ == AST_VAR);
  Parser_skip (self, 1); /* Skip the mapsto symbol.  */
  ASTNodeId body = Parser_parse_expr (self);
  if (is_invalid_ast_node_id (body))
    {
      return get_invalid_ast_node_id ();
    }
  Vec_ASTNodeId vars = Vec_ASTNodeId_with_capacity (1);
  Vec_ASTNodeId_push (&vars, var);
  Vec_ASTNodeId types = Vec_ASTNodeId_with_capacity (1);
  Vec_ASTNodeId_push (&types, get_null_ast_node_id ());
  return ASTNodeManager_push_lambda (
      self->ast_mgr_, Parser_span_from_last_node (self, cbegin, body), vars,
      types, body);
}

static ASTNodeId
invalid_and_drop_ids_1 (Vec_ASTNodeId *v)
{
  Vec_ASTNodeId_drop (v);
  return get_invalid_ast_node_id ();
}

static ASTNodeId
invalid_and_drop_ids_2 (Vec_ASTNodeId *v1, Vec_ASTNodeId *v2)
{
  Vec_ASTNodeId_drop (v1);
  Vec_ASTNodeId_drop (v2);
  return get_invalid_ast_node_id ();
}

static ASTNodeId
invalid_and_drop_ids_3 (Vec_ASTNodeId *v1, Vec_ASTNodeId *v2,
                        Vec_ASTNodeId *v3)
{
  Vec_ASTNodeId_drop (v1);
  Vec_ASTNodeId_drop (v2);
  Vec_ASTNodeId_drop (v3);
  return get_invalid_ast_node_id ();
}

static bool
Parser_parse_push_var_and_type (Parser *self, Vec_ASTNodeId *vars,
                                Vec_ASTNodeId *types)
{
  ASTNodeId var = Parser_parse_var (self);
  if (is_invalid_ast_node_id (var))
    {
      return false;
    }
  ASTNodeId type = get_null_ast_node_id ();
  if (Parser_seeing (self, TOKEN_COLON))
    {
      Parser_skip (self, 1);
      type = Parser_parse_type (self);
      if (is_invalid_ast_node_id (type))
        {
          return false;
        }
    }
  Vec_ASTNodeId_push (vars, var);
  Vec_ASTNodeId_push (types, type);
  return true;
}

static ASTNodeId
Parser_parse_lambda_paren_params (Parser *self)
{
  assert (Parser_seeing (self, TOKEN_LPAREN));
  const char *cbegin = Parser_cursor_cbegin (self);
  Parser_skip (self, 1);
  Vec_ASTNodeId vars = Vec_ASTNodeId_new ();
  Vec_ASTNodeId types = Vec_ASTNodeId_new ();
  /* The caller guarantees the parentheses are matched.  */
  if (!Parser_seeing (self, TOKEN_RPAREN)
      && !Parser_parse_push_var_and_type (self, &vars, &types))
    {
      return invalid_and_drop_ids_2 (&vars, &types);
    }
  while (!Parser_seeing (self, TOKEN_RPAREN))
    {
      if (!Parser_expect_and_skip (self, TOKEN_COMMA)
          || !Parser_parse_push_var_and_type (self, &vars, &types))
        {
          return invalid_and_drop_ids_2 (&vars, &types);
        }
    }
  Parser_skip (self, 1);
  assert (Parser_seeing (self, TOKEN_PLUS_GT));
  Parser_skip (self, 1);
  ASTNodeId body = Parser_parse_expr (self);
  if (is_invalid_ast_node_id (body))
    {
      return invalid_and_drop_ids_2 (&vars, &types);
    }
  return ASTNodeManager_push_lambda (
      self->ast_mgr_, Parser_span_from_last_node (self, cbegin, body), vars,
      types, body);
}

static const Token *
find_matched_rparen (const Token *lparen)
{
  assert (lparen->kind_ == TOKEN_LPAREN);
  const Token *token = lparen + 1;
  while (true)
    {
      switch (token->kind_)
        {
        case TOKEN_RPAREN:
          {
            return token;
          }
        case TOKEN_LPAREN:
          {
            token = find_matched_rparen (token);
            if (!token)
              {
                return NULL;
              }
            token++;
            break;
          }
        case TOKEN_EOF:
          {
            return NULL;
          }
        default:
          token++;
          break;
        }
    }
}

static bool
Parser_parse_push_expr (Parser *self, Vec_ASTNodeId *exprs)
{
  ASTNodeId expr = Parser_parse_expr (self);
  if (is_invalid_ast_node_id (expr))
    {
      return false;
    }
  Vec_ASTNodeId_push (exprs, expr);
  return true;
}

static ASTNodeId
Parser_parse_tuple (Parser *self)
{
  assert (Parser_seeing (self, TOKEN_LPAREN));
  const Token *rparen = find_matched_rparen (self->cursor_);
  if (!rparen)
    {
      DiagnosticManager_diagnose_unclosed_dilimiter (self->diag_mgr_,
                                                     self->cursor_->span_);
      return get_invalid_ast_node_id ();
    }
  const char *cbegin = Parser_cursor_cbegin (self);
  Parser_skip (self, 1);
  Vec_ASTNodeId args = Vec_ASTNodeId_new ();
  if (!Parser_seeing (self, TOKEN_RPAREN)
      && !Parser_parse_push_expr (self, &args))
    {
      return invalid_and_drop_ids_1 (&args);
    }
  while (!Parser_seeing (self, TOKEN_RPAREN))
    {
      if (!Parser_expect_and_skip (self, TOKEN_COMMA)
          || !Parser_parse_push_expr (self, &args))
        {
          return invalid_and_drop_ids_1 (&args);
        }
    }
  const char *cend = Span_cend (&self->cursor_->span_);
  Parser_skip (self, 1);
  return ASTNodeManager_push_tuple (self->ast_mgr_,
                                    Span_new (cbegin, cend - cbegin), args);
}

static ASTNodeId
Parser_parse_name (Parser *self)
{
  assert (Parser_seeing (self, TOKEN_NAME));
  if (Parser_seeing_after (self, 1, TOKEN_COLON))
    {
      return Parser_parse_lambda_single_param_with_type (self);
    }
  else if (Parser_seeing_after (self, 1, TOKEN_PLUS_GT))
    {
      return Parser_parse_lambda_single_param_without_type (self);
    }
  else
    {
      return Parser_parse_var (self);
    }
}

static ASTNodeId
Parser_parse_lparen (Parser *self)
{
  assert (Parser_seeing (self, TOKEN_LPAREN));
  const Token *rparen = find_matched_rparen (self->cursor_);
  if (!rparen)
    {
      DiagnosticManager_diagnose_unclosed_dilimiter (self->diag_mgr_,
                                                     self->cursor_->span_);
      return get_invalid_ast_node_id ();
    }
  const Token *rparen_next = rparen + 1;
  if (rparen_next->kind_ == TOKEN_PLUS_GT)
    {
      return Parser_parse_lambda_paren_params (self);
    }
  return Parser_parse_tuple (self);
}

static bool
Parser_parse_push_var_type_init (Parser *self, Vec_ASTNodeId *vars,
                                 Vec_ASTNodeId *types, Vec_ASTNodeId *inits)
{
  ASTNodeId var = Parser_parse_var (self);
  if (is_invalid_ast_node_id (var))
    {
      return false;
    }
  ASTNodeId type = get_null_ast_node_id ();
  if (Parser_seeing (self, TOKEN_COLON))
    {
      Parser_skip (self, 1);
      type = Parser_parse_type (self);
      if (is_invalid_ast_node_id (type))
        {
          return false;
        }
    }
  if (!Parser_expect_and_skip (self, TOKEN_EQ))
    {
      return false;
    }
  ASTNodeId init = Parser_parse_expr (self);
  if (is_invalid_ast_node_id (init))
    {
      return false;
    }
  Vec_ASTNodeId_push (vars, var);
  Vec_ASTNodeId_push (types, type);
  Vec_ASTNodeId_push (inits, init);
  return true;
}

static ASTNodeId
Parser_parse_let (Parser *self)
{
  const char *cbegin = Parser_cursor_cbegin (self);
  if (!Parser_expect_and_skip (self, TOKEN_LET))
    {
      return get_invalid_ast_node_id ();
    }
  Vec_ASTNodeId vars = Vec_ASTNodeId_new ();
  Vec_ASTNodeId types = Vec_ASTNodeId_new ();
  Vec_ASTNodeId inits = Vec_ASTNodeId_new ();
  if (!Parser_parse_push_var_type_init (self, &vars, &types, &inits))
    {
      return invalid_and_drop_ids_3 (&vars, &types, &inits);
    }
  while (!Parser_seeing (self, TOKEN_IN))
    {
      if (!Parser_expect_and_skip (self, TOKEN_COMMA)
          || Parser_parse_push_var_type_init (self, &vars, &types, &inits))
        {
          return invalid_and_drop_ids_3 (&vars, &types, &inits);
        }
    }
  Parser_skip (self, 1);
  ASTNodeId body = Parser_parse_expr (self);
  if (is_invalid_ast_node_id (body))
    {
      return invalid_and_drop_ids_3 (&vars, &types, &inits);
    }
  return ASTNodeManager_push_let (
      self->ast_mgr_, Parser_span_from_last_node (self, cbegin, body), vars,
      types, inits, body);
}

static ASTNodeId
Parser_parse_expr_before_tuple (Parser *self)
{
  switch (self->cursor_->kind_)
    {
    case TOKEN_INVALID:
      {
        DiagnosticManager_diagnose_invalid_token (self->diag_mgr_,
                                                  self->cursor_->span_);
        return get_invalid_ast_node_id ();
      }
    case TOKEN_FALSE:
      {
        ASTNodeId id = ASTNodeManager_push_lit (
            self->ast_mgr_, self->cursor_->span_, AST_LIT_FALSE);
        Parser_skip (self, 1);
        return id;
      }
    case TOKEN_TRUE:
      {
        ASTNodeId id = ASTNodeManager_push_lit (
            self->ast_mgr_, self->cursor_->span_, AST_LIT_TRUE);
        Parser_skip (self, 1);
        return id;
      }
    case TOKEN_INTEGER:
      {
        ASTNodeId id = ASTNodeManager_push_lit (
            self->ast_mgr_, self->cursor_->span_, AST_LIT_INTEGER);
        Parser_skip (self, 1);
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
        return Parser_parse_name (self);
      }
    case TOKEN_LPAREN:
      {
        return Parser_parse_lparen (self);
      }
    default:
      {
        DiagnosticManager_diagnose_expected_node (
            self->diag_mgr_, self->cursor_->span_, AST_EXPR);
        return get_invalid_ast_node_id ();
      }
    }
}

static Span
Parser_span_between_node (const Parser *self, ASTNodeId first, ASTNodeId last)
{
  const char *cbegin
      = Span_cbegin (&ASTNodeManager_get_node (self->ast_mgr_, first)->span_);
  return Span_new (
      cbegin,
      Span_cend (&ASTNodeManager_get_node (self->ast_mgr_, last)->span_)
          - cbegin);
}

static ASTNodeId
Parser_parse_expr_before_operator (Parser *self)
{
  ASTNodeId base = Parser_parse_expr_before_tuple (self);
  if (is_invalid_ast_node_id (base))
    {
      return get_invalid_ast_node_id ();
    }
  if (!Parser_seeing (self, TOKEN_LPAREN))
    {
      return base;
    }
  ASTNodeId tuple = Parser_parse_tuple (self);
  if (is_invalid_ast_node_id (tuple))
    {
      return get_invalid_ast_node_id ();
    }
  return ASTNodeManager_push_call (
      self->ast_mgr_, Parser_span_between_node (self, base, tuple), base,
      tuple);
}

static int
op_precedence (enum TokenKind op)
{
  switch (op)
    {
    case TOKEN_PLUS:
    case TOKEN_HYPHEN:
      return 6;
    case TOKEN_ASTERISK:
    case TOKEN_SLASH:
      return 7;
    default:
      return 0;
    }
}

static ASTNodeId
drop_shunting_yard_stacks (Vec_ASTNodeId *expr_stack, Vec_TokenKind *op_stack)
{
  Vec_ASTNodeId_drop (expr_stack);
  Vec_TokenKind_drop (op_stack);
  return get_invalid_ast_node_id ();
}

static ASTNodeId
Parser_parse_expr (Parser *self)
{
  ASTNodeId expr = Parser_parse_expr_before_operator (self);
  if (is_invalid_ast_node_id (expr))
    {
      return get_invalid_ast_node_id ();
    }
  if (!(Parser_seeing (self, TOKEN_PLUS) || Parser_seeing (self, TOKEN_HYPHEN)
        || Parser_seeing (self, TOKEN_ASTERISK)
        || Parser_seeing (self, TOKEN_SLASH)))
    {
      return expr;
    }
  /* Shunting yard algorithm.  */
  Vec_ASTNodeId expr_stack = Vec_ASTNodeId_new ();
  Vec_ASTNodeId_push (&expr_stack, expr);
  Vec_TokenKind op_stack = Vec_TokenKind_new ();
  while (Parser_seeing (self, TOKEN_PLUS) || Parser_seeing (self, TOKEN_HYPHEN)
         || Parser_seeing (self, TOKEN_ASTERISK)
         || Parser_seeing (self, TOKEN_SLASH))
    {
      enum TokenKind op = self->cursor_->kind_;
      Parser_skip (self, 1);
      while (!Vec_TokenKind_is_empty (&op_stack)
             && op_precedence (op)
                    < op_precedence (*(Vec_TokenKind_cend (&op_stack) - 1)))
        {
          enum TokenKind op_old = Vec_TokenKind_pop (&op_stack);
          ASTNodeId right = Vec_ASTNodeId_pop (&expr_stack);
          ASTNodeId left = Vec_ASTNodeId_pop (&expr_stack);
          Vec_ASTNodeId_push (&expr_stack,
                              ASTNodeManager_push_binary (
                                  self->ast_mgr_,
                                  Parser_span_between_node (self, left, right),
                                  op_old, left, right));
        }
      Vec_TokenKind_push (&op_stack, op);
      expr = Parser_parse_expr_before_operator (self);
      if (is_invalid_ast_node_id (expr))
        {
          return drop_shunting_yard_stacks (&expr_stack, &op_stack);
        }
      Vec_ASTNodeId_push (&expr_stack, expr);
    }
  while (!Vec_TokenKind_is_empty (&op_stack))
    {
      enum TokenKind op = Vec_TokenKind_pop (&op_stack);
      ASTNodeId right = Vec_ASTNodeId_pop (&expr_stack);
      ASTNodeId left = Vec_ASTNodeId_pop (&expr_stack);
      Vec_ASTNodeId_push (&expr_stack,
                          ASTNodeManager_push_binary (
                              self->ast_mgr_,
                              Parser_span_between_node (self, left, right), op,
                              left, right));
    }
  assert (Vec_ASTNodeId_len (&expr_stack) == 1);
  expr = Vec_ASTNodeId_pop (&expr_stack);
  Vec_ASTNodeId_drop (&expr_stack);
  Vec_TokenKind_drop (&op_stack);
  return expr;
}

ASTNodeId
Parser_parse (Parser *self)
{
  ASTNodeId id = Parser_parse_expr (self);
  if (!is_invalid_ast_node_id (id) && !Token_is_eof (self->cursor_))
    {
      DiagnosticManager_diagnose_unexpected_token (self->diag_mgr_,
                                                   self->cursor_->span_);
      return get_invalid_ast_node_id ();
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

static size_t
parse_num_diags (const char *content)
{
  ParserTest parser_test;
  ParserTest_init (&parser_test, content);
  Parser_parse (ParserTest_borrow_parser (&parser_test));
  size_t res = DiagnosticManager_num_total (
      ParserTest_get_diagnostic_manager (&parser_test));
  ParserTest_drop (&parser_test);
  return res;
}

NEO_TEST (test_parse_true_00)
{
  ASSERT_U64_EQ (parse_num_diags ("true"), 0);
  ASSERT_U64_EQ (parse_num_diags ("true\n"), 0);
}

NEO_TEST (test_parse_if_00)
{
  ASSERT_U64_EQ (parse_num_diags ("if true then\n"
                                  "    true\n"
                                  "else\n"
                                  "    false\n"),
                 0);
}

NEO_TEST (test_parse_let_00)
{
  ASSERT_U64_EQ (parse_num_diags ("let x: Bool = true in x"), 0);
  ASSERT_U64_EQ (
      parse_num_diags ("let x: Bool = if true then true else false in x"), 0);
  ASSERT_U64_EQ (parse_num_diags ("let x = true in\n"
                                  "let y = false in\n"
                                  "if true then x else y"),
                 0);
  ASSERT_U64_EQ (
      parse_num_diags ("let f = (x, y, z) +> if x then y else z in f"), 0);
}

NEO_TEST (test_parse_let_01)
{
  ASSERT_U64_EQ (
      parse_num_diags ("let x = false, y = true in if false then x else y"),
      0);
}

NEO_TEST (test_parse_lambda_00)
{
  ASSERT_U64_EQ (parse_num_diags ("x +> x"), 0);
  ASSERT_U64_EQ (parse_num_diags ("x: Bool +> x"), 0);
  ASSERT_U64_EQ (parse_num_diags ("()+>true"), 0);
  ASSERT_U64_EQ (parse_num_diags ("(x)+>true"), 0);
  ASSERT_U64_EQ (parse_num_diags ("(x: Bool)+>true"), 0);
  ASSERT_U64_EQ (parse_num_diags ("(x: Bool, y)+>true"), 0);
}

NEO_TEST (test_parse_call_00)
{
  ASSERT_U64_EQ (parse_num_diags ("let f = n +> if zero(n) then\n"
                                  "        1\n"
                                  "    else n * f(n - 1)\n"
                                  "in f(10)"),
                 0);
}

NEO_TESTS (parser_tests, test_parse_true_00, test_parse_if_00,
           test_parse_let_00, test_parse_let_01, test_parse_lambda_00,
           test_parse_call_00)
#endif
