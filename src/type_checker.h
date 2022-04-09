/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_TYPE_CHECKER_H
#define NEO_TYPE_CHECKER_H

#include "ast_node.h"
#include "diagnostic.h"
#include "type.h"

typedef struct TypeChecker
{
  const ASTNodeManager *ast_mgr_;
  DiagnosticManager *diag_mgr_;
  TypeManager type_mgr_;
  Vec_TypeId node_type_map_;
} TypeChecker;

TypeChecker TypeChecker_new (const ASTNodeManager *ast_mgr,
                             DiagnosticManager *diag_mgr);
void TypeChecker_drop (TypeChecker *self);
TypeId TypeChecker_typeof (TypeChecker *self, ASTNodeId node_id);

#ifdef TESTS
#include "test.h"
Tests type_checker_tests ();
#endif

#endif
