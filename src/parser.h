/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_PARSER_H
#define NEO_PARSER_H

#include "ast_node.h"
#include "diagnostic.h"
#include "lexer.h"
#include "token.h"

typedef struct Parser
{
  const Vec_Token *tokens_;
  DiagnosticManager *diag_mgr_;
  ASTNodeManager *ast_mgr_;
  const Token *cursor_;
} Parser;

Parser Parser_new (const Vec_Token *tokens, DiagnosticManager *diag_mgr,
                   ASTNodeManager *ast_mgr);
ASTNodeId Parser_parse (Parser *self);

#ifdef TESTS
#include "test.h"
Tests parser_tests ();
#endif

#endif
