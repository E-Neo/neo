/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_AST_NODE_H
#define NEO_AST_NODE_H

#include <stdint.h>

#include "array_macro.h"
#include "span.h"
#include "vec_macro.h"

typedef uint32_t ASTNodeId;
typedef struct ASTNode ASTNode;

NEO_DECL_VEC (ASTNodeId, ASTNodeId);
NEO_DECL_VEC (ASTNode, ASTNode)

enum ASTKind
{
#define NEO_ASTKIND(NAME, UNUSED) AST_##NAME,
#include "ast_kind.def"
#undef NEO_ASTKIND
};

NEO_DECL_ARRAY (ASTKind, enum ASTKind)

typedef struct ASTBlock
{
  Vec_ASTNodeId exprs_;
} ASTBlock;

typedef struct ASTIfThenElse
{
  ASTNodeId if_expr_;
  ASTNodeId then_block_;
  ASTNodeId else_block_;
} ASTIfThenElse;

typedef struct ASTLet
{
  ASTNodeId pattern_;
  ASTNodeId expr_;
} ASTLet;

typedef struct ASTNode
{
  enum ASTKind kind_;
  Span span_;
  union
  {
    ASTBlock block_;
    ASTIfThenElse if_then_else_;
    ASTLet let_;
  };
} ASTNode;

typedef struct ASTNodeManager
{
  Vec_ASTNode nodes_;
} ASTNodeManager;

ASTNodeManager ASTNodeManager_new ();
void ASTNodeManager_drop (ASTNodeManager *self);
const Vec_ASTNode *ASTNodeManager_get_nodes (const ASTNodeManager *self);
ASTNodeId ASTNodeManager_get_id (const ASTNodeManager *self,
                                 const ASTNode *node);
const ASTNode *ASTNodeManager_get_node (const ASTNodeManager *self,
                                        ASTNodeId id);
const Span *ASTNodeManager_get_span (const ASTNodeManager *self, ASTNodeId id);
ASTNodeId ASTNodeManager_push_invalid (ASTNodeManager *self);
ASTNodeId ASTNodeManager_push_void (ASTNodeManager *self, Span span);
ASTNodeId ASTNodeManager_push_identifier_pattern (ASTNodeManager *self,
                                                  Span span);
ASTNodeId ASTNodeManager_push_lit (ASTNodeManager *self, Span span,
                                   enum ASTKind kind);
ASTNodeId ASTNodeManager_push_block (ASTNodeManager *self, Span span,
                                     Vec_ASTNodeId exprs);
ASTNodeId ASTNodeManager_push_if_then_else (ASTNodeManager *self, Span span,
                                            ASTNodeId if_expr,
                                            ASTNodeId then_block,
                                            ASTNodeId else_block);
ASTNodeId ASTNodeManager_push_let (ASTNodeManager *self, Span span,
                                   ASTNodeId pattern, ASTNodeId expr);
ASTNodeId ASTNodeManager_push_var (ASTNodeManager *self, Span span);

#endif
