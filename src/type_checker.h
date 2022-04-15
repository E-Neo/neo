/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_TYPE_CHECKER_H
#define NEO_TYPE_CHECKER_H

#include "ast_node.h"
#include "diagnostic.h"
#include "type.h"
#include "vec_macro.h"

typedef struct ASTNodeIdToTypeIdMap
{
  Vec_TypeId map_;
} ASTNodeIdToTypeIdMap;

void ASTNodeIdToTypeIdMap_drop (ASTNodeIdToTypeIdMap *self);
TypeId ASTNodeIdToTypeIdMap_get (const ASTNodeIdToTypeIdMap *self,
                                 ASTNodeId node_id);

typedef struct TypeChecker
{
  const ASTNodeManager *ast_mgr_;
  DiagnosticManager *diag_mgr_;
  TypeManager *type_mgr_;
  ASTNodeIdToTypeIdMap map_;
} TypeChecker;

TypeChecker TypeChecker_new (const ASTNodeManager *ast_mgr,
                             DiagnosticManager *diag_mgr,
                             TypeManager *type_mgr);
ASTNodeIdToTypeIdMap TypeChecker_check (TypeChecker *self, ASTNodeId node_id);

#endif
