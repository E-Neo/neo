/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include <stdio.h>

#include "ast_node.h"
#include "diagnostic.h"
#include "lexer.h"
#include "parser.h"
#include "span.h"
#include "string.h"
#include "token.h"
#include "vec.h"

#define INTEGER_BUFFER_SIZE (64)

static void
print_copyright ()
{
  puts ("Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.");
}

static void
print_prompt ()
{
  puts ("-------------------------------------------------------------------");
}

static String
Position_serialize (const Position *self)
{
  String json = String_from_cstring ("{\"line\":");
  String_push_u64 (&json, Position_get_line (self));
  String_push_cstring (&json, ",\"column\":");
  String_push_u64 (&json, Position_get_column (self));
  String_push_cstring (&json, "}");
  return json;
}

static String
SourceFile_serialize_span (const SourceFile *self, const Span *span)
{
  String json = String_from_cstring ("{\"path\":\"");
  String_push_string (&json, SourceFile_get_path (self));
  String_push_cstring (&json, "\",\"begin\":");
  Position begin = SourceFile_lookup_position (self, Span_cbegin (span));
  String begin_json = Position_serialize (&begin);
  String_push_string (&json, &begin_json);
  String_drop (&begin_json);
  String_push_cstring (&json, ",\"end\":");
  Position end = SourceFile_lookup_position (self, Span_cend (span) - 1);
  String end_json = Position_serialize (&end);
  String_push_string (&json, &end_json);
  String_drop (&end_json);
  String_push_cstring (&json, ",\"content\":\"");
  String_push_carray (&json, Span_cbegin (span), Span_len (span));
  String_push_cstring (&json, "\"}");
  return json;
}

static String
SourceFile_serialize_token (const SourceFile *self, const Token *token)
{
  String json;
  switch (token->kind_)
    {
#define NEO_TOKEN(T)                                                          \
  case TOKEN_##T:                                                             \
    {                                                                         \
      json = String_from_cstring ("{\"kind\":\"TOKEN_" #T "\",\"span\":");    \
      break;                                                                  \
    }
#include "token.def"
#define NEO_TOKEN_LIT(T, UNUSED) NEO_TOKEN (T)
#include "token_lit.def"
#undef NEO_TOKEN_LIT
#undef NEO_TOKEN
    }
  String span_json = SourceFile_serialize_span (self, &token->span_);
  String_push_string (&json, &span_json);
  String_drop (&span_json);
  String_push_cstring (&json, "}");
  return json;
}

static void
SourceFile_display_token (const SourceFile *self, const Token *token)
{
  String json = SourceFile_serialize_token (self, token);
  printf ("%.*s", (int)String_len (&json), String_cbegin (&json));
  String_drop (&json);
}

static void
ASTNode_display (const ASTNodeManager *ast_mgr, const ASTNode *node)
{
  switch (node->kind_)
    {
#define NEO_ASTKIND(NAME, UNUSED)                                             \
  case AST_##NAME:                                                            \
    {                                                                         \
      printf ("{\"id\":%u,\"kind\":\"" #NAME "\",\"span\":\"%.*s\"",          \
              ASTNodeManager_get_id (ast_mgr, node),                          \
              (int)Span_len (&node->span_), Span_cbegin (&node->span_));      \
      break;                                                                  \
    }
#include "ast_kind.def"
#undef NEO_ASTKIND
    }
  switch (node->kind_)
    {
    case AST_IF_THEN_ELSE:
      {
        printf (",\"if_expr\":%u,\"then_expr\":%u,\"else_expr\":%u",
                node->if_then_else_.if_expr_, node->if_then_else_.then_block_,
                node->if_then_else_.else_block_);
      }
    default:
      break;
    }
  printf ("}");
}

static void
ASTNodeManager_display (const ASTNodeManager *ast_mgr)
{
  for (const ASTNode *node
       = Vec_ASTNode_cbegin (ASTNodeManager_get_nodes (ast_mgr));
       node < Vec_ASTNode_cend (ASTNodeManager_get_nodes (ast_mgr)); node++)
    {
      ASTNode_display (ast_mgr, node);
      puts ("");
    }
}

int
main ()
{
  print_copyright ();
  print_prompt ();
  int ch = 0;
  String input_buf = String_new ();
  while ((ch = getchar ()) != EOF)
    {
      String_push (&input_buf, ch);
      if (ch == '\n')
        {
          Span span
              = Span_new (String_cbegin (&input_buf), String_len (&input_buf));
          SourceFile file
              = SourceFile_new (String_from_cstring ("<stdio>"), input_buf);
          /* Lexical analysis: */
          Lexer lexer = Lexer_new (&span);
          Vec_Token tokens = Vec_Token_new ();
          Option_Token opt_token = Lexer_next (&lexer);
          puts ("Tokens:");
          while (Option_Token_is_some (&opt_token))
            {
              Token token = Option_Token_unwrap (&opt_token);
              SourceFile_display_token (&file, &token);
              puts ("");
              Vec_Token_push (&tokens, token);
              opt_token = Lexer_next (&lexer);
            }
          DiagnosticManager diag_mgr = DiagnosticManager_new (&file);
          ASTNodeManager ast_mgr = ASTNodeManager_new ();
          Parser parser = Parser_new (&tokens, &diag_mgr, &ast_mgr);
          printf ("ASTNodeId = %u\n", Parser_parse (&parser));
          Vec_Token_drop (&tokens);
          puts ("AST Nodes:");
          ASTNodeManager_display (&ast_mgr);
          ASTNodeManager_drop (&ast_mgr);
          DiagnosticManager_drop (&diag_mgr);
          /* Clear input buffer and print prompt: */
          SourceFile_drop (&file);
          input_buf = String_new ();
          print_prompt ();
        }
    }
  return 0;
}