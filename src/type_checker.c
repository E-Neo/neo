/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "type_checker.h"

#include <assert.h>
#include <stddef.h>

#include "ast_node.h"
#include "diagnostic.h"
#include "span.h"
#include "type.h"
#include "vec_macro.h"

void
ASTNodeIdToTypeIdMap_drop (ASTNodeIdToTypeIdMap *self)
{
  Vec_TypeId_drop (&self->map_);
}

TypeId
ASTNodeIdToTypeIdMap_get (const ASTNodeIdToTypeIdMap *self, ASTNodeId node_id)
{
  assert (node_id < Vec_TypeId_len (&self->map_));
  return Vec_TypeId_cbegin (&self->map_)[node_id];
}

static ASTNodeIdToTypeIdMap
ASTNodeIdToTypeIdMap_new (size_t len)
{
  Vec_TypeId map = Vec_TypeId_with_capacity (len);
  Vec_TypeId_resize (&map, len, TYPE_UNKNOWN);
  return (ASTNodeIdToTypeIdMap){ .map_ = map };
}

static void
ASTNodeIdToTypeIdMap_set (ASTNodeIdToTypeIdMap *self, ASTNodeId node_id,
                          TypeId type_id)
{
  assert (node_id < Vec_TypeId_len (&self->map_));
  Vec_TypeId_begin (&self->map_)[node_id] = type_id;
}

TypeChecker
TypeChecker_new (const ASTNodeManager *ast_mgr, DiagnosticManager *diag_mgr,
                 TypeManager *type_mgr)
{
  return (TypeChecker){ .ast_mgr_ = ast_mgr,
                        .diag_mgr_ = diag_mgr,
                        .type_mgr_ = type_mgr,
                        .map_ = ASTNodeIdToTypeIdMap_new (Vec_ASTNode_len (
                            ASTNodeManager_get_nodes (ast_mgr))) };
}

typedef struct TypeEnvEntry
{
  Span name_;
  TypeId type_id_;
} TypeEnvEntry;

NEO_DECL_VEC (TypeEnvEntry, TypeEnvEntry)
NEO_IMPL_VEC (TypeEnvEntry, TypeEnvEntry)

static TypeId
TypeChecker_set_map (TypeChecker *self, ASTNodeId node_id, TypeId type_id)
{
  ASTNodeIdToTypeIdMap_set (&self->map_, node_id, type_id);
  return type_id;
}

static TypeId TypeChecker_typeof (TypeChecker *self, ASTNodeId node_id,
                                  Vec_TypeEnvEntry *env);

static TypeId
TypeChecker_typeof_if_then_else (TypeChecker *self, ASTNodeId node_id,
                                 Vec_TypeEnvEntry *env)
{
  const ASTNode *node = ASTNodeManager_get_node (self->ast_mgr_, node_id);
  assert (node->kind_ == AST_IF_THEN_ELSE);
  TypeId if_expr_type_id
      = TypeChecker_typeof (self, node->if_then_else_.if_expr_, env);
  if (!TypeManager_is_bool (self->type_mgr_, if_expr_type_id))
    {
      DiagnosticManager_diagnose_if_expr_not_bool (
          self->diag_mgr_,
          ASTNodeManager_get_node (self->ast_mgr_,
                                   node->if_then_else_.if_expr_)
              ->span_,
          TypeManager_to_string (self->type_mgr_, if_expr_type_id));
      return TypeChecker_set_map (self, node_id,
                                  TypeManager_get_invalid (self->type_mgr_));
    }
  TypeId then_expr_type_id
      = TypeChecker_typeof (self, node->if_then_else_.then_expr_, env);
  if (TypeManager_is_invalid (self->type_mgr_, then_expr_type_id))
    {
      return TypeChecker_set_map (self, node_id,
                                  TypeManager_get_invalid (self->type_mgr_));
    }
  TypeId else_expr_type_id
      = TypeChecker_typeof (self, node->if_then_else_.else_expr_, env);
  if (TypeManager_is_invalid (self->type_mgr_, else_expr_type_id))
    {
      return TypeChecker_set_map (self, node_id,
                                  TypeManager_get_invalid (self->type_mgr_));
    }
  if (!TypeManager_are_equal (self->type_mgr_, then_expr_type_id,
                              else_expr_type_id))
    {
      DiagnosticManager_diagnose_expr_types_not_equal (
          self->diag_mgr_,
          ASTNodeManager_get_node (self->ast_mgr_,
                                   node->if_then_else_.then_expr_)
              ->span_,
          TypeManager_to_string (self->type_mgr_, then_expr_type_id),
          ASTNodeManager_get_node (self->ast_mgr_,
                                   node->if_then_else_.else_expr_)
              ->span_,
          TypeManager_to_string (self->type_mgr_, else_expr_type_id));
      return TypeChecker_set_map (self, node_id,
                                  TypeManager_get_invalid (self->type_mgr_));
    }
  return TypeChecker_set_map (self, node_id, then_expr_type_id);
}

static TypeId
TypeChecker_typeof_type (TypeChecker *self, ASTNodeId node_id,
                         Vec_TypeEnvEntry *env)
{
  const ASTNode *node = ASTNodeManager_get_node (self->ast_mgr_, node_id);
  assert (node->kind_ == AST_TYPE);
  const TypeEnvEntry *ptr = Vec_TypeEnvEntry_cend (env);
  while (ptr-- > Vec_TypeEnvEntry_cbegin (env))
    {
      if (!Span_cmp (&node->span_, &ptr->name_))
        {
          return TypeChecker_set_map (self, node_id, ptr->type_id_);
        }
    }
  DiagnosticManager_diagnose_invalid_type (self->diag_mgr_, node->span_);
  return TypeChecker_set_map (self, node_id,
                              TypeManager_get_invalid (self->type_mgr_));
}

static TypeId
TypeChecker_typeof_let (TypeChecker *self, ASTNodeId node_id,
                        Vec_TypeEnvEntry *env)
{
  const ASTNode *node = ASTNodeManager_get_node (self->ast_mgr_, node_id);
  assert (node->kind_ == AST_LET);
  TypeId var_type_id = TypeChecker_typeof (self, node->let_.type_, env);
  if (TypeManager_is_invalid (self->type_mgr_, var_type_id))
    {
      return TypeChecker_set_map (self, node_id,
                                  TypeManager_get_invalid (self->type_mgr_));
    }
  TypeId expr_type_id = TypeChecker_typeof (self, node->let_.expr_, env);
  if (TypeManager_is_invalid (self->type_mgr_, expr_type_id))
    {
      Vec_TypeEnvEntry_pop (env);
      return TypeChecker_set_map (self, node_id,
                                  TypeManager_get_invalid (self->type_mgr_));
    }
  if (!TypeManager_are_equal (self->type_mgr_, var_type_id, expr_type_id))
    {
      DiagnosticManager_diagnose_expr_types_not_equal (
          self->diag_mgr_,
          ASTNodeManager_get_node (self->ast_mgr_, node->let_.var_)->span_,
          TypeManager_to_string (self->type_mgr_, var_type_id),
          ASTNodeManager_get_node (self->ast_mgr_, node->let_.expr_)->span_,
          TypeManager_to_string (self->type_mgr_, expr_type_id));
      Vec_TypeEnvEntry_pop (env);
      return TypeChecker_set_map (self, node_id,
                                  TypeManager_get_invalid (self->type_mgr_));
    }
  Vec_TypeEnvEntry_push (
      env,
      (TypeEnvEntry){
          .name_
          = ASTNodeManager_get_node (self->ast_mgr_, node->let_.var_)->span_,
          .type_id_ = var_type_id });
  TypeId body_type_id = TypeChecker_typeof (self, node->let_.body_, env);
  if (TypeManager_is_invalid (self->type_mgr_, body_type_id))
    {
      Vec_TypeEnvEntry_pop (env);
      return TypeChecker_set_map (self, node_id,
                                  TypeManager_get_invalid (self->type_mgr_));
    }
  Vec_TypeEnvEntry_pop (env);
  return TypeChecker_set_map (self, node_id, body_type_id);
}

static TypeId
TypeChecker_typeof_var (TypeChecker *self, ASTNodeId node_id,
                        Vec_TypeEnvEntry *env)
{
  const ASTNode *node = ASTNodeManager_get_node (self->ast_mgr_, node_id);
  assert (node->kind_ == AST_VAR);
  const TypeEnvEntry *ptr = Vec_TypeEnvEntry_cend (env);
  while (ptr-- > Vec_TypeEnvEntry_cbegin (env))
    {
      if (!Span_cmp (&node->span_, &ptr->name_))
        {
          return TypeChecker_set_map (self, node_id, ptr->type_id_);
        }
    }
  DiagnosticManager_diagnose_var_not_bound (self->diag_mgr_, node->span_);
  return TypeChecker_set_map (self, node_id,
                              TypeManager_get_invalid (self->type_mgr_));
}

static TypeId
TypeChecker_typeof (TypeChecker *self, ASTNodeId node_id,
                    Vec_TypeEnvEntry *env)
{
  if (!TypeManager_is_unknown (
          self->type_mgr_, ASTNodeIdToTypeIdMap_get (&self->map_, node_id)))
    {
      return ASTNodeIdToTypeIdMap_get (&self->map_, node_id);
    }
  const ASTNode *node = ASTNodeManager_get_node (self->ast_mgr_, node_id);
  switch (node->kind_)
    {
    case AST_LIT_FALSE:
    case AST_LIT_TRUE:
      {
        return TypeChecker_set_map (self, node_id,
                                    TypeManager_get_bool (self->type_mgr_));
      }
    case AST_IF_THEN_ELSE:
      {
        return TypeChecker_typeof_if_then_else (self, node_id, env);
      }
    case AST_TYPE:
      {
        return TypeChecker_typeof_type (self, node_id, env);
      }
    case AST_LET:
      {
        return TypeChecker_typeof_let (self, node_id, env);
      }
    case AST_VAR:
      {
        return TypeChecker_typeof_var (self, node_id, env);
      }
    default:
      {
        return TypeChecker_set_map (self, node_id,
                                    TypeManager_get_invalid (self->type_mgr_));
      }
    }
}

ASTNodeIdToTypeIdMap
TypeChecker_check (TypeChecker *self, ASTNodeId node_id)
{
  Vec_TypeEnvEntry env = Vec_TypeEnvEntry_new ();
  Vec_TypeEnvEntry_push (
      &env,
      (TypeEnvEntry){ .name_ = Span_from_cstring ("Bool"),
                      .type_id_ = TypeManager_get_bool (self->type_mgr_) });
  TypeChecker_typeof (self, node_id, &env);
  Vec_TypeEnvEntry_drop (&env);
  return self->map_;
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
  TypeManager type_mgr_;
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
  self->type_mgr_ = TypeManager_new ();
  self->type_checker_
      = TypeChecker_new (&self->ast_mgr_, &self->diag_mgr_, &self->type_mgr_);
}

static void
TypeCheckerTest_drop (TypeCheckerTest *self)
{
  SourceFile_drop (&self->file_);
  ASTNodeManager_drop (&self->ast_mgr_);
  DiagnosticManager_drop (&self->diag_mgr_);
  TypeManager_drop (&self->type_mgr_);
}

static ASTNodeId
TypeCheckerTest_get_node_id (const TypeCheckerTest *self)
{
  return self->node_id_;
}

static const TypeManager *
TypeCheckerTest_get_type_manager (const TypeCheckerTest *self)
{
  return &self->type_mgr_;
}

static TypeChecker *
TypeCheckerTest_borrow_type_checker (TypeCheckerTest *self)
{
  return &self->type_checker_;
}

NEO_TEST (test_check_true_00)
{
  TypeCheckerTest tester;
  TypeCheckerTest_init (&tester, "true");
  TypeChecker *checker = TypeCheckerTest_borrow_type_checker (&tester);
  ASTNodeIdToTypeIdMap node_type_map
      = TypeChecker_check (checker, TypeCheckerTest_get_node_id (&tester));
  ASSERT_U64_EQ (
      ASTNodeIdToTypeIdMap_get (&node_type_map,
                                TypeCheckerTest_get_node_id (&tester)),
      TypeManager_get_bool (TypeCheckerTest_get_type_manager (&tester)));
  ASTNodeIdToTypeIdMap_drop (&node_type_map);
  TypeCheckerTest_drop (&tester);
}

NEO_TEST (test_check_if_00)
{
  TypeCheckerTest tester;
  TypeCheckerTest_init (&tester, "if true then true else false");
  TypeChecker *checker = TypeCheckerTest_borrow_type_checker (&tester);
  ASTNodeIdToTypeIdMap node_type_map
      = TypeChecker_check (checker, TypeCheckerTest_get_node_id (&tester));
  ASSERT_U64_EQ (
      ASTNodeIdToTypeIdMap_get (&node_type_map,
                                TypeCheckerTest_get_node_id (&tester)),
      TypeManager_get_bool (TypeCheckerTest_get_type_manager (&tester)));
  ASTNodeIdToTypeIdMap_drop (&node_type_map);
  TypeCheckerTest_drop (&tester);
}

NEO_TEST (test_check_let_00)
{
  TypeCheckerTest tester;
  TypeCheckerTest_init (&tester, "let x: Bool = true in\n"
                                 "let y: Bool = false in\n"
                                 "if true then x else y");
  TypeChecker *checker = TypeCheckerTest_borrow_type_checker (&tester);
  ASTNodeIdToTypeIdMap node_type_map
      = TypeChecker_check (checker, TypeCheckerTest_get_node_id (&tester));
  ASSERT_U64_EQ (
      ASTNodeIdToTypeIdMap_get (&node_type_map,
                                TypeCheckerTest_get_node_id (&tester)),
      TypeManager_get_bool (TypeCheckerTest_get_type_manager (&tester)));
  ASTNodeIdToTypeIdMap_drop (&node_type_map);
  TypeCheckerTest_drop (&tester);
}

NEO_TESTS (type_checker_tests, test_check_true_00, test_check_if_00,
           test_check_let_00)
#endif
