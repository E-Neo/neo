/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "ast_node.h"

#include <assert.h>
#include <stdbool.h>

#include "array_macro.h"
#include "span.h"
#include "token.h"
#include "vec_macro.h"

NEO_IMPL_VEC (ASTNodeId, ASTNodeId)
NEO_IMPL_VEC (ASTNode, ASTNode)

NEO_IMPL_ARRAY (ASTKind, enum ASTKind)

ASTNodeId
get_null_ast_node_id ()
{
  return 0;
}

ASTNodeId
get_invalid_ast_node_id ()
{
  return 1;
}

bool
is_null_ast_node_id (ASTNodeId id)
{
  return id == 0;
}

bool
is_invalid_ast_node_id (ASTNodeId id)
{
  return id == 1;
}

ASTNodeManager
ASTNodeManager_new ()
{
  Vec_ASTNode nodes = Vec_ASTNode_new ();
  Vec_ASTNode_push (&nodes, (ASTNode){ .kind_ = AST_NULL });
  Vec_ASTNode_push (&nodes, (ASTNode){ .kind_ = AST_INVALID });
  return (ASTNodeManager){ .nodes_ = nodes };
}

void
ASTNodeManager_drop (ASTNodeManager *self)
{
  for (ASTNode *ptr = Vec_ASTNode_begin (&self->nodes_);
       ptr < Vec_ASTNode_end (&self->nodes_); ptr++)
    {
      switch (ptr->kind_)
        {
        case AST_LET:
          {
            Vec_ASTNodeId_drop (&ptr->let_.vars_);
            Vec_ASTNodeId_drop (&ptr->let_.types_);
            Vec_ASTNodeId_drop (&ptr->let_.inits_);
            break;
          }
        case AST_LAMBDA:
          {
            Vec_ASTNodeId_drop (&ptr->lambda_.vars_);
            Vec_ASTNodeId_drop (&ptr->lambda_.types_);
            break;
          }
        case AST_TUPLE:
          {
            Vec_ASTNodeId_drop (&ptr->tuple_.args_);
            break;
          }
        default:
          break;
        }
    }
  Vec_ASTNode_drop (&self->nodes_);
}

const Vec_ASTNode *
ASTNodeManager_get_nodes (const ASTNodeManager *self)
{
  return &self->nodes_;
}

ASTNodeId
ASTNodeManager_get_id (const ASTNodeManager *self, const ASTNode *node)
{
  assert (node >= Vec_ASTNode_cbegin (&self->nodes_));
  assert (node < Vec_ASTNode_cend (&self->nodes_));
  return node - Vec_ASTNode_cbegin (&self->nodes_);
}

const ASTNode *
ASTNodeManager_get_node (const ASTNodeManager *self, ASTNodeId id)
{
  assert (id < Vec_ASTNode_len (&self->nodes_));
  return Vec_ASTNode_cbegin (&self->nodes_) + id;
}

const Span *
ASTNodeManager_get_span (const ASTNodeManager *self, ASTNodeId id)
{
  assert (id < Vec_ASTNode_len (&self->nodes_));
  return &(Vec_ASTNode_cbegin (&self->nodes_) + id)->span_;
}

static ASTNodeId
ASTNodeManager_get_next_id (const ASTNodeManager *self)
{
  return Vec_ASTNode_len (&self->nodes_);
}

ASTNodeId
ASTNodeManager_push_lit (ASTNodeManager *self, Span span, enum ASTKind kind)
{
  ASTNodeId id = ASTNodeManager_get_next_id (self);
  Vec_ASTNode_push (&self->nodes_, (ASTNode){
                                       .kind_ = kind,
                                       .span_ = span,
                                   });
  return id;
}

ASTNodeId
ASTNodeManager_push_if_then_else (ASTNodeManager *self, Span span,
                                  ASTNodeId if_expr, ASTNodeId then_expr,
                                  ASTNodeId else_expr)
{
  ASTNodeId id = ASTNodeManager_get_next_id (self);
  Vec_ASTNode_push (&self->nodes_, (ASTNode){ .kind_ = AST_IF_THEN_ELSE,
                                              .span_ = span,
                                              .if_then_else_ = (ASTIfThenElse){
                                                  .if_expr_ = if_expr,
                                                  .then_expr_ = then_expr,
                                                  .else_expr_ = else_expr } });
  return id;
}

ASTNodeId
ASTNodeManager_push_var (ASTNodeManager *self, Span span)
{
  ASTNodeId id = ASTNodeManager_get_next_id (self);
  Vec_ASTNode_push (&self->nodes_,
                    (ASTNode){ .kind_ = AST_VAR, .span_ = span });
  return id;
}

ASTNodeId
ASTNodeManager_push_type (ASTNodeManager *self, Span span)
{
  ASTNodeId id = ASTNodeManager_get_next_id (self);
  Vec_ASTNode_push (&self->nodes_,
                    (ASTNode){ .kind_ = AST_TYPE, .span_ = span });
  return id;
}

ASTNodeId
ASTNodeManager_push_let (ASTNodeManager *self, Span span, Vec_ASTNodeId vars,
                         Vec_ASTNodeId types, Vec_ASTNodeId inits,
                         ASTNodeId body)
{
  ASTNodeId id = ASTNodeManager_get_next_id (self);
  Vec_ASTNode_push (&self->nodes_,
                    (ASTNode){ .kind_ = AST_LET,
                               .span_ = span,
                               .let_ = (ASTLet){ .vars_ = vars,
                                                 .types_ = types,
                                                 .inits_ = inits,
                                                 .body_ = body } });
  return id;
}

ASTNodeId
ASTNodeManager_push_lambda (ASTNodeManager *self, Span span,
                            Vec_ASTNodeId vars, Vec_ASTNodeId types,
                            ASTNodeId body)
{
  ASTNodeId id = ASTNodeManager_get_next_id (self);
  Vec_ASTNode_push (&self->nodes_,
                    (ASTNode){ .kind_ = AST_LAMBDA,
                               .span_ = span,
                               .lambda_ = (ASTLambda){ .vars_ = vars,
                                                       .types_ = types,
                                                       .body_ = body } });
  return id;
}

static enum ASTKind
op_to_ast (enum TokenKind op)
{
  switch (op)
    {
    case TOKEN_PLUS:
      return AST_ADD;
    case TOKEN_HYPHEN:
      return AST_SUB;
    case TOKEN_ASTERISK:
      return AST_MUL;
    case TOKEN_SLASH:
      return AST_DIV;
    default:
      {
        assert (false);
        return AST_INVALID;
      }
    }
}

ASTNodeId
ASTNodeManager_push_tuple (ASTNodeManager *self, Span span, Vec_ASTNodeId args)
{
  ASTNodeId id = ASTNodeManager_get_next_id (self);
  Vec_ASTNode_push (&self->nodes_,
                    (ASTNode){ .kind_ = AST_TUPLE,
                               .span_ = span,
                               .tuple_ = (ASTTuple){ .args_ = args } });
  return id;
}

ASTNodeId
ASTNodeManager_push_call (ASTNodeManager *self, Span span, ASTNodeId base,
                          ASTNodeId tuple)
{
  ASTNodeId id = ASTNodeManager_get_next_id (self);
  Vec_ASTNode_push (
      &self->nodes_,
      (ASTNode){ .kind_ = AST_CALL,
                 .span_ = span,
                 .call_ = (ASTCall){ .base_ = base, .tuple_ = tuple } });
  return id;
}

ASTNodeId
ASTNodeManager_push_unary (ASTNodeManager *self, Span span, enum TokenKind op,
                           ASTNodeId expr)
{
  ASTNodeId id = ASTNodeManager_get_next_id (self);
  Vec_ASTNode_push (&self->nodes_,
                    (ASTNode){ .kind_ = op_to_ast (op),
                               .span_ = span,
                               .unary_ = (ASTUnary){ .expr_ = expr } });
  return id;
}

ASTNodeId
ASTNodeManager_push_binary (ASTNodeManager *self, Span span, enum TokenKind op,
                            ASTNodeId left, ASTNodeId right)
{
  ASTNodeId id = ASTNodeManager_get_next_id (self);
  Vec_ASTNode_push (
      &self->nodes_,
      (ASTNode){ .kind_ = op_to_ast (op),
                 .span_ = span,
                 .binary_ = (ASTBinary){ .left_ = left, .right_ = right } });
  return id;
}
