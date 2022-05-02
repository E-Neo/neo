/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "diagnostic.h"

#include <assert.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>

#include "ast_node.h"
#include "span.h"
#include "string.h"
#include "token.h"
#include "vec_macro.h"

#define ESC_DEFAULT "\e[0m"
#define ESC_BOLD "\e[1m"
#define ESC_RED "\e[1;31m"
#define ESC_GREEN "\e[1;32m"
#define ESC_YELLOW "\e[1;33m"
#define ESC_BLUE "\e[1;34m"

NEO_IMPL_VEC (SpanInfo, SpanInfo)
NEO_IMPL_VEC (DiagnosticId, DiagnosticId)
NEO_IMPL_VEC (Diagnostic, Diagnostic)

static String
Span_fmt (const Span *self)
{
  if (self && Span_len (self))
    {
      String output = String_from_cstring ("`");
      String_push_carray (&output, Span_cbegin (self), Span_len (self));
      String_push_cstring (&output, "`");
      return output;
    }
  return String_from_cstring ("<eof>");
}

static SpanInfo
SpanInfo_new (Span span, String label)
{
  return (SpanInfo){ .span_ = span, .label_ = label };
}

static void
SpanInfo_drop (SpanInfo *self)
{
  String_drop (&self->label_);
}

static void
SpanInfo_set_label (SpanInfo *self, String label)
{
  String_drop (&self->label_);
  self->label_ = label;
}

static Diagnostic
Diagnostic_new (enum DiagnosticName name, Span span)
{
  switch (name)
    {
#define NEO_DIAGNOSTIC(NAME, LEVEL)                                           \
  case DIAGNOSTIC_##NAME:                                                     \
    {                                                                         \
      Vec_SpanInfo span_infos = Vec_SpanInfo_new ();                          \
      Vec_SpanInfo_push (&span_infos, SpanInfo_new (span, String_new ()));    \
      return (Diagnostic){ .name_ = DIAGNOSTIC_##NAME,                        \
                           .level_ = DIAG_LEVEL_##LEVEL,                      \
                           .message_ = String_new (),                         \
                           .span_infos_ = span_infos,                         \
                           .children_ = Vec_DiagnosticId_new () };            \
    }
#include "diagnostic.def"
#undef NEO_DIAGNOSTIC
    }
  return (Diagnostic){};
}

static void
Diagnostic_drop (Diagnostic *self)
{
  String_drop (&self->message_);
  for (SpanInfo *span_info = Vec_SpanInfo_begin (&self->span_infos_);
       span_info < Vec_SpanInfo_end (&self->span_infos_); span_info++)
    {
      SpanInfo_drop (span_info);
    }
  Vec_SpanInfo_drop (&self->span_infos_);
  Vec_DiagnosticId_drop (&self->children_);
}

static void
Diagnostic_set_message (Diagnostic *self, String message)
{
  String_drop (&self->message_);
  self->message_ = message;
}

static void
Diagnostic_push_span_info (Diagnostic *self, SpanInfo span_info)
{
  Vec_SpanInfo_push (&self->span_infos_, span_info);
}

static void
Diagnostic_set_span_info_label (Diagnostic *self, size_t span_info_id,
                                String label)
{
  assert (span_info_id < Vec_SpanInfo_len (&self->span_infos_));
  SpanInfo_set_label (Vec_SpanInfo_begin (&self->span_infos_), label);
}

DiagnosticManager
DiagnosticManager_new (const SourceFile *file)
{
  return (DiagnosticManager){ .file_ = file,
                              .colored_ = true,
                              .display_ = true,
                              .diagnostics_ = Vec_Diagnostic_new () };
}

void
DiagnosticManager_drop (DiagnosticManager *self)
{
  for (Diagnostic *diag = Vec_Diagnostic_begin (&self->diagnostics_);
       diag < Vec_Diagnostic_end (&self->diagnostics_); diag++)
    {
      Diagnostic_drop (diag);
    }
  Vec_Diagnostic_drop (&self->diagnostics_);
}

size_t
DiagnosticManager_num_total (const DiagnosticManager *self)
{
  return Vec_Diagnostic_len (&self->diagnostics_);
}

void
DiagnosticManager_set_colored (DiagnosticManager *self, bool colored)
{
  self->colored_ = colored;
}

void
DiagnosticManager_set_display (DiagnosticManager *self, bool display)
{
  self->display_ = display;
}

const Diagnostic *
DiagnosticManager_get (const DiagnosticManager *self, DiagnosticId id)
{
  return Vec_Diagnostic_cbegin (&self->diagnostics_) + id;
}

static DiagnosticId
DiagnosticManager_push (DiagnosticManager *self, Diagnostic diag)
{
  DiagnosticId id = Vec_Diagnostic_len (&self->diagnostics_);
  Vec_Diagnostic_push (&self->diagnostics_, diag);
  return id;
}

static const char *
level_to_cstring (enum DiagnosticLevel level, bool is_colored)
{
  switch (level)
    {
    case DIAG_LEVEL_ERROR:
      return is_colored ? ESC_RED "error" ESC_DEFAULT : "error";
    case DIAG_LEVEL_WARNING:
      return is_colored ? ESC_RED "warning" ESC_DEFAULT : "warning";
    case DIAG_LEVEL_NOTE:
      return is_colored ? ESC_YELLOW "note" ESC_DEFAULT : "note";
    }
  return NULL;
}

static const char *
arrow_to_cstring (bool is_colored)
{
  return is_colored ? ESC_BLUE "-->" ESC_DEFAULT : "-->";
}

static const char *
vertical_bar_to_cstring (bool is_colored)
{
  return is_colored ? ESC_BLUE "|" ESC_DEFAULT : "|";
}

static const char *
caret_to_cstring (bool is_colored)
{
  return is_colored ? ESC_RED "^" ESC_DEFAULT : "^";
}

static size_t
get_num_digits (size_t n)
{
  size_t num_digits = 0;
  while (n)
    {
      num_digits++;
      n /= 10;
    }
  return num_digits;
}

static String
DiagnosticManager_fmt_span_info (const DiagnosticManager *self,
                                 const SpanInfo *span_info)
{
  String output = String_new ();
  const SourceFile *file = self->file_;
  const Span *span = &span_info->span_;
  Position begin_pos = SourceFile_lookup_position (file, Span_cbegin (span));
  Position end_pos = SourceFile_lookup_position (
      file, Span_len (span) ? Span_cend (span) - 1 : Span_cend (span));
  size_t begin_pos_line = Position_get_line (&begin_pos);
  if (!begin_pos_line)
    {
      return output;
    }
  size_t begin_pos_column = Position_get_column (&begin_pos);
  size_t end_pos_line = Position_get_line (&end_pos);
  assert (begin_pos_line == end_pos_line);
  size_t line_number_digits = get_num_digits (end_pos_line);
  String_push_repeat (&output, ' ', line_number_digits);
  String_push_cstring (&output, arrow_to_cstring (self->colored_));
  String_push (&output, ' ');
  String_push_string (&output, SourceFile_get_path (file));
  String_push_cstring (&output, ":");
  String_push_u64 (&output, begin_pos_line);
  String_push_cstring (&output, ":");
  String_push_u64 (&output, begin_pos_column + 1);
  String_push_cstring (&output, "\n");
  String_push_repeat (&output, ' ', line_number_digits + 1);
  String_push_cstring (&output, vertical_bar_to_cstring (self->colored_));
  String_push_cstring (&output, "\n");
  if (self->colored_)
    {
      String_push_cstring (&output, ESC_BLUE);
    }
  String_push_u64 (&output, begin_pos_line);
  if (self->colored_)
    {
      String_push_cstring (&output, ESC_DEFAULT);
    }
  String_push (&output, ' ');
  String_push_cstring (&output, vertical_bar_to_cstring (self->colored_));
  String_push (&output, ' ');
  Span line = SourceFile_get_line (file, begin_pos_line);
  String_push_carray (&output, Span_cbegin (&line), Span_len (&line));
  if (Span_len (&line) == 0 || *(Span_cend (&line) - 1) != '\n')
    {
      String_push (&output, '\n');
    }
  String_push_repeat (&output, ' ', line_number_digits + 1);
  String_push_cstring (&output, vertical_bar_to_cstring (self->colored_));
  String_push_repeat (&output, ' ', begin_pos_column + 1);
  String_push_cstring_repeat (&output, caret_to_cstring (self->colored_),
                              Span_len (span) ? Span_len (span) : 1);
  if (String_len (&span_info->label_))
    {
      String_push (&output, ' ');
      String_push_string (&output, &span_info->label_);
    }
  String_push (&output, '\n');
  return output;
}

static String
DiagnosticManager_fmt_diagnostic (const DiagnosticManager *self,
                                  DiagnosticId id)
{
  const Diagnostic *diag = DiagnosticManager_get (self, id);
  String output = String_new ();
  String_push_cstring (&output,
                       level_to_cstring (diag->level_, self->colored_));
  if (self->colored_)
    {
      String_push_cstring (&output, ESC_BOLD);
    }
  String_push_cstring (&output, ": ");
  String_push_string (&output, &diag->message_);
  if (self->colored_)
    {
      String_push_cstring (&output, ESC_DEFAULT);
    }
  String_push_cstring (&output, "\n");
  for (const SpanInfo *span_info = Vec_SpanInfo_cbegin (&diag->span_infos_);
       span_info < Vec_SpanInfo_cend (&diag->span_infos_); span_info++)
    {
      String span_info_output
          = DiagnosticManager_fmt_span_info (self, span_info);
      String_push_string (&output, &span_info_output);
      String_drop (&span_info_output);
    }
  return output;
}

static void
DiagnosticManager_display (const DiagnosticManager *self, DiagnosticId id)
{
  if (!self->display_)
    {
      return;
    }
  String output = DiagnosticManager_fmt_diagnostic (self, id);
  fprintf (stderr, "%.*s", (int)String_len (&output), String_cbegin (&output));
  String_drop (&output);
}

void
DiagnosticManager_diagnose_invalid_token (DiagnosticManager *self, Span span)
{
  Diagnostic diag = Diagnostic_new (DIAGNOSTIC_INVALID_TOKEN, span);
  String message = String_from_cstring ("invalid token: ");
  String span_output = Span_fmt (&span);
  String_push_string (&message, &span_output);
  String_drop (&span_output);
  Diagnostic_set_message (&diag, message);
  DiagnosticId id = DiagnosticManager_push (self, diag);
  DiagnosticManager_display (self, id);
}

void
DiagnosticManager_diagnose_expected_tokens_or_nodes (DiagnosticManager *self,
                                                     Span span,
                                                     Array_TokenKind tokens,
                                                     Array_ASTKind nodes)
{
  enum DiagnosticName diag_name;
  if (Array_TokenKind_is_empty (&tokens) && Array_ASTKind_is_empty (&nodes))
    {
      return;
    }
  else if (Array_TokenKind_is_empty (&tokens))
    {
      diag_name = DIAGNOSTIC_EXPECTED_NODE;
    }
  else if (Array_ASTKind_is_empty (&nodes))
    {
      diag_name = DIAGNOSTIC_EXPECTED_TOKEN;
    }
  else
    {
      diag_name = DIAGNOSTIC_EXPECTED_TOKENS_OR_NODES;
    }
  size_t num_candidates
      = Array_TokenKind_len (&tokens) + Array_ASTKind_len (&nodes);
  Diagnostic diag = Diagnostic_new (diag_name, span);
  String message = String_from_cstring ("expected ");
  size_t num_candidates_pushed = 0;
  for (const enum TokenKind *token = Array_TokenKind_cbegin (&tokens);
       token < Array_TokenKind_cend (&tokens); token++)
    {
      switch (*token)
        {
#define NEO_TOKEN_LIT(NAME, LITERAL)                                          \
  case TOKEN_##NAME:                                                          \
    {                                                                         \
      String_push_cstring (&message, "`" #LITERAL "`");                       \
      break;                                                                  \
    }
#include "token_lit.def"
#undef NEO_TOKEN_LIT
#define NEO_TOKEN(NAME)                                                       \
  case TOKEN_##NAME:                                                          \
    {                                                                         \
      String_push_cstring (&message, "<" #NAME ">");                          \
      break;                                                                  \
    }
#include "token.def"
#undef NEO_TOKEN
        }
      String_push_cstring (&message, ", ");
      num_candidates_pushed++;
      if (num_candidates_pushed == num_candidates - 1)
        {
          String_push_cstring (&message, "or ");
        }
    }
  for (const enum ASTKind *node = Array_ASTKind_cbegin (&nodes);
       node < Array_ASTKind_cend (&nodes); node++)
    {
      switch (*node)
        {
#define NEO_ASTKIND(NAME, LABEL)                                              \
  case AST_##NAME:                                                            \
    {                                                                         \
      String_push_cstring (&message, "<");                                    \
      String_push_cstring (&message, #LABEL);                                 \
      String_push_cstring (&message, ">");                                    \
      break;                                                                  \
    }
#include "ast_kind.def"
#undef NEO_ASTKIND
        }
      String_push_cstring (&message, ", ");
      num_candidates_pushed++;
      if (num_candidates_pushed == num_candidates - 1)
        {
          String_push_cstring (&message, "or ");
        }
    }
  String_push_cstring (&message, "found ");
  String span_output = Span_fmt (&span);
  String_push_string (&message, &span_output);
  String_drop (&span_output);
  Diagnostic_set_message (&diag, message);
  DiagnosticId id = DiagnosticManager_push (self, diag);
  DiagnosticManager_display (self, id);
}

void
DiagnosticManager_diagnose_expected_token (DiagnosticManager *self, Span span,
                                           enum TokenKind token)
{
  return DiagnosticManager_diagnose_expected_tokens_or_nodes (
      self, span, Array_TokenKind_new (&token, 1),
      Array_ASTKind_new (NULL, 0));
}

void
DiagnosticManager_diagnose_expected_node (DiagnosticManager *self, Span span,
                                          enum ASTKind node)
{
  return DiagnosticManager_diagnose_expected_tokens_or_nodes (
      self, span, Array_TokenKind_new (NULL, 0), Array_ASTKind_new (&node, 1));
}

void
DiagnosticManager_diagnose_unexpected_token (DiagnosticManager *self,
                                             Span span)
{
  Diagnostic diag = Diagnostic_new (DIAGNOSTIC_UNEXPECTED_TOKEN, span);
  String message = String_from_cstring ("unexpected token: ");
  String span_output = Span_fmt (&span);
  String_push_string (&message, &span_output);
  String_drop (&span_output);
  Diagnostic_set_message (&diag, message);
  DiagnosticId id = DiagnosticManager_push (self, diag);
  DiagnosticManager_display (self, id);
}

void
DiagnosticManager_diagnose_invalid_type (DiagnosticManager *self, Span span)
{
  Diagnostic diag = Diagnostic_new (DIAGNOSTIC_INVALID_TYPE, span);
  String message = String_from_cstring ("invalid type: ");
  String span_output = Span_fmt (&span);
  String_push_string (&message, &span_output);
  String_drop (&span_output);
  Diagnostic_set_message (&diag, message);
  DiagnosticId id = DiagnosticManager_push (self, diag);
  DiagnosticManager_display (self, id);
}

void
DiagnosticManager_diagnose_if_expr_not_bool (DiagnosticManager *self,
                                             Span span, String type)
{
  Diagnostic diag = Diagnostic_new (DIAGNOSTIC_INVALID_TYPE, span);
  String message
      = String_from_cstring ("condition is not a subtype of Bool: ");
  String span_output = Span_fmt (&span);
  String_push_string (&message, &span_output);
  String_drop (&span_output);
  Diagnostic_set_message (&diag, message);
  String label = String_from_cstring ("is of type `");
  String_push_string (&label, &type);
  String_push (&label, '`');
  String_drop (&type);
  Diagnostic_set_span_info_label (&diag, 0, label);
  DiagnosticId id = DiagnosticManager_push (self, diag);
  DiagnosticManager_display (self, id);
}

void
DiagnosticManager_diagnose_expr_types_not_equal (DiagnosticManager *self,
                                                 Span span1, String type1,
                                                 Span span2, String type2)
{
  Diagnostic diag = Diagnostic_new (DIAGNOSTIC_INVALID_TYPE, span1);
  String message
      = String_from_cstring ("types of then and else are not equal");
  Diagnostic_set_message (&diag, message);
  String then_label = String_from_cstring ("is of type `");
  String_push_string (&then_label, &type1);
  String_push (&then_label, '`');
  String_drop (&type1);
  Diagnostic_set_span_info_label (&diag, 0, then_label);
  String else_label = String_from_cstring ("is of type `");
  String_push_string (&else_label, &type2);
  String_push (&else_label, '`');
  String_drop (&type2);
  Diagnostic_push_span_info (&diag, SpanInfo_new (span2, else_label));
  DiagnosticId id = DiagnosticManager_push (self, diag);
  DiagnosticManager_display (self, id);
}

void
DiagnosticManager_diagnose_var_not_bound (DiagnosticManager *self, Span span)
{
  Diagnostic diag = Diagnostic_new (DIAGNOSTIC_INVALID_TYPE, span);
  String message = String_from_cstring ("the variable is not bound: ");
  String span_output = Span_fmt (&span);
  String_push_string (&message, &span_output);
  String_drop (&span_output);
  Diagnostic_set_message (&diag, message);
  DiagnosticId id = DiagnosticManager_push (self, diag);
  DiagnosticManager_display (self, id);
}
