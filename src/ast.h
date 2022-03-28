/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_AST_NODE_H
#define NEO_AST_NODE_H

#include "span.h"
#include "vec.h"

enum ASTKind
{
  AST_INVALID,
  AST_LET_VAR,
  AST_LIT_TRUE,
  AST_LIT_FALSE
};

typedef struct LitExpr
{
  Span lit_;
} LitExpr;

typedef struct ASTLetVar
{
  Span name_;
} LetVar;

typedef struct LetFunc
{
  Span name_;
  Vec_ASTNode args_;
} LetFunc;

typedef union ASTNodeUnion
{
  LitExpr lit_expr_;
  LetVar let_var_;
  LetFunc let_func;
} ASTNodeUnion;

typedef struct ASTNode
{
  enum ASTKind kind_;
  ASTNodeUnion data_;
} ASTNode;

#endif
