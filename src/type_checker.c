/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "type_checker.h"

#include <assert.h>
#include <stddef.h>

#include "ast_node.h"
#include "diagnostic.h"
#include "span.h"
#include "string.h"
#include "token.h"
#include "type.h"

TypeChecker
TypeChecker_new (const ASTNodeManager *ast_mgr, DiagnosticManager *diag_mgr)
{
  size_t len = Vec_ASTNode_len (ASTNodeManager_get_nodes (ast_mgr));
  Vec_TypeId node_type_map = Vec_TypeId_with_capacity (len);
  Vec_TypeId_resize (&node_type_map, len, 0);
  return (TypeChecker){ .ast_mgr_ = ast_mgr,
                        .diag_mgr_ = diag_mgr,
                        .type_mgr_ = TypeManager_new (),
                        .node_type_map_ = node_type_map };
}

void
TypeChecker_drop (TypeChecker *self)
{
  TypeManager_drop (&self->type_mgr_);
  Vec_TypeId_drop (&self->node_type_map_);
}

static TypeId
TypeChecker_typeof_get_cache (const TypeChecker *self, ASTNodeId node_id)
{
  return Vec_TypeId_cbegin (&self->node_type_map_)[node_id];
}

static TypeId
TypeChecker_typeof_set_cache (TypeChecker *self, ASTNodeId node_id,
                              TypeId type_id)
{
  Vec_TypeId_begin (&self->node_type_map_)[node_id] = type_id;
  return type_id;
}

static TypeId
TypeChecker_typeof_block (TypeChecker *self, ASTNodeId node_id)
{
  const ASTNode *block = ASTNodeManager_get_node (self->ast_mgr_, node_id);
  assert (block->kind_ == AST_BLOCK);
  TypeId expr_type_id = TYPE_VOID;
  for (const ASTNodeId *expr_id = Vec_ASTNodeId_cbegin (&block->block_.exprs_);
       expr_id < Vec_ASTNodeId_cend (&block->block_.exprs_); expr_id++)
    {
      expr_type_id = TypeChecker_typeof (self, *expr_id);
      if (TypeManager_get_type (&self->type_mgr_, expr_type_id)->kind_
          == TYPE_INVALID)
        {
          return TypeChecker_typeof_set_cache (self, node_id, TYPE_INVALID);
        }
    }
  return TypeChecker_typeof_set_cache (self, node_id, expr_type_id);
}

TypeId
TypeChecker_typeof (TypeChecker *self, ASTNodeId node_id)
{
  assert (node_id
          < Vec_ASTNode_len (ASTNodeManager_get_nodes (self->ast_mgr_)));
  if (TypeChecker_typeof_get_cache (self, node_id))
    {
      return TypeChecker_typeof_get_cache (self, node_id);
    }
  const ASTNode *node = ASTNodeManager_get_node (self->ast_mgr_, node_id);
  switch (node->kind_)
    {
    case AST_VOID:
      {
        return TypeChecker_typeof_set_cache (self, node_id, TYPE_VOID);
      }
    case AST_LIT_FALSE:
      {
        return TypeChecker_typeof_set_cache (self, node_id, TYPE_FALSE);
      }
    case AST_LIT_TRUE:
      {
        return TypeChecker_typeof_set_cache (self, node_id, TYPE_TRUE);
      }
    case AST_BLOCK:
      {
        return TypeChecker_typeof_block (self, node_id);
      }
    default:
      {
        DiagnosticManager_diagnose_invalid_type (self->diag_mgr_, node->span_);
        return TYPE_INVALID;
      }
    }
}

#ifdef TESTS
#include "test.h"

#include "lexer.h"
#include "parser.h"

typedef struct TypeCheckerTest
{
  SourceFile file_;
  ASTNodeManager ast_mgr_;
  DiagnosticManager diag_mgr_;
  ASTNodeId node_id_;
  TypeChecker type_checker_;
} TypeCheckerTest;

static void
TypeCheckerTest_init (TypeCheckerTest *self, const char *content)
{
  self->file_ = SourceFile_new (String_from_cstring ("test"),
                                String_from_cstring (content));
  Span content_span = Span_from_string (SourceFile_get_content (&self->file_));
  Lexer lexer = Lexer_new (&content_span);
  Vec_Token tokens = Vec_Token_new ();
  Token token;
  do
    {
      token = Lexer_next (&lexer);
      Vec_Token_push (&tokens, token);
    }
  while (!Token_is_eof (&token));
  self->ast_mgr_ = ASTNodeManager_new ();
  self->diag_mgr_ = DiagnosticManager_new (&self->file_);
  Parser parser = Parser_new (&tokens, &self->diag_mgr_, &self->ast_mgr_);
  self->node_id_ = Parser_parse (&parser);
  Vec_Token_drop (&tokens);
  self->type_checker_ = TypeChecker_new (&self->ast_mgr_, &self->diag_mgr_);
}

static void
TypeCheckerTest_drop (TypeCheckerTest *self)
{
  SourceFile_drop (&self->file_);
  ASTNodeManager_drop (&self->ast_mgr_);
  DiagnosticManager_drop (&self->diag_mgr_);
  TypeChecker_drop (&self->type_checker_);
}

static ASTNodeId
TypeCheckerTest_get_node_id (const TypeCheckerTest *self)
{
  return self->node_id_;
}

static TypeChecker *
TypeCheckerTest_borrow_type_checker (TypeCheckerTest *self)
{
  return &self->type_checker_;
}

NEO_TEST (test_typeof_true_00)
{
  TypeCheckerTest tester;
  TypeCheckerTest_init (&tester, "true");
  ASSERT_U64_EQ (
      TypeChecker_typeof (TypeCheckerTest_borrow_type_checker (&tester),
                          TypeCheckerTest_get_node_id (&tester)),
      TYPE_TRUE);
  TypeCheckerTest_drop (&tester);
}

NEO_TEST (test_typeof_block_00)
{
  TypeCheckerTest tester;
  TypeCheckerTest_init (&tester, "{true; false}");
  ASSERT_U64_EQ (
      TypeChecker_typeof (TypeCheckerTest_borrow_type_checker (&tester),
                          TypeCheckerTest_get_node_id (&tester)),
      TYPE_FALSE);
  TypeCheckerTest_drop (&tester);
}

NEO_TEST (test_typeof_block_01)
{
  TypeCheckerTest tester;
  TypeCheckerTest_init (&tester, "{true; false;}");
  ASSERT_U64_EQ (
      TypeChecker_typeof (TypeCheckerTest_borrow_type_checker (&tester),
                          TypeCheckerTest_get_node_id (&tester)),
      TYPE_VOID);
  TypeCheckerTest_drop (&tester);
}

NEO_TESTS (type_checker_tests, test_typeof_true_00, test_typeof_block_00,
           test_typeof_block_01)
#endif
