/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "ast_node.h"

#include <assert.h>

#include "array_macro.h"
#include "span.h"
#include "vec_macro.h"

NEO_IMPL_VEC (ASTNodeId, ASTNodeId)
NEO_IMPL_VEC (ASTNode, ASTNode)

NEO_IMPL_ARRAY (ASTKind, enum ASTKind)

ASTNodeManager
ASTNodeManager_new ()
{
  return (ASTNodeManager){ .nodes_ = Vec_ASTNode_new () };
}

void
ASTNodeManager_drop (ASTNodeManager *self)
{
  for (ASTNode *node = Vec_ASTNode_begin (&self->nodes_);
       node < Vec_ASTNode_end (&self->nodes_); node++)
    {
      switch (node->kind_)
        {
        case AST_BLOCK:
          {
            Vec_ASTNodeId_drop (&node->block_.exprs_);
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
ASTNodeManager_push_invalid (ASTNodeManager *self)
{
  ASTNodeId id = ASTNodeManager_get_next_id (self);
  Vec_ASTNode_push (&self->nodes_, (ASTNode){ .kind_ = AST_INVALID });
  return id;
}

ASTNodeId
ASTNodeManager_push_void (ASTNodeManager *self, Span span)
{
  ASTNodeId id = ASTNodeManager_get_next_id (self);
  Vec_ASTNode_push (&self->nodes_,
                    (ASTNode){ .kind_ = AST_VOID, .span_ = span });
  return id;
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
ASTNodeManager_push_block (ASTNodeManager *self, Span span,
                           Vec_ASTNodeId exprs)
{
  ASTNodeId id = ASTNodeManager_get_next_id (self);
  Vec_ASTNode_push (&self->nodes_,
                    (ASTNode){ .kind_ = AST_BLOCK,
                               .span_ = span,
                               .block_ = (ASTBlock){ .exprs_ = exprs } });
  return id;
}

ASTNodeId
ASTNodeManager_push_if_then_else (ASTNodeManager *self, Span span,
                                  ASTNodeId if_expr, ASTNodeId then_expr,
                                  ASTNodeId else_expr)
{
  ASTNodeId id = Vec_ASTNode_len (&self->nodes_);
  Vec_ASTNode_push (&self->nodes_,
                    (ASTNode){ .kind_ = AST_IF_THEN_ELSE,
                               .span_ = span,
                               .if_then_else_ = (ASTIfThenElse){
                                   .if_expr_ = if_expr,
                                   .then_block_ = then_expr,
                                   .else_block_ = else_expr } });
  return id;
}
