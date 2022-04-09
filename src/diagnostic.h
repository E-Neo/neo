/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_DIAGNOSTIC_H
#define NEO_DIAGNOSTIC_H

#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>

#include "ast_node.h"
#include "span.h"
#include "string.h"
#include "token.h"
#include "vec_macro.h"

enum DiagnosticLevel
{
  DIAG_LEVEL_ERROR,
  DIAG_LEVEL_WARNING,
  DIAG_LEVEL_NOTE
};

enum DiagnosticName
{
#define NEO_DIAGNOSTIC(NAME, UNUSED) DIAGNOSTIC_##NAME,
#include "diagnostic.def"
#undef NEO_DIAGNOSTIC
};

typedef struct SpanInfo
{
  Span span_;
  String label_;
} SpanInfo;

NEO_DECL_VEC (SpanInfo, SpanInfo)

typedef uint32_t DiagnosticId;

NEO_DECL_VEC (DiagnosticId, DiagnosticId)

typedef struct Diagnostic
{
  enum DiagnosticName name_;
  enum DiagnosticLevel level_;
  String message_;
  Vec_SpanInfo span_infos_;
  Vec_DiagnosticId children_;
} Diagnostic;

NEO_DECL_VEC (Diagnostic, Diagnostic)

typedef struct DiagnosticManager
{
  const SourceFile *file_;
  bool colored_;
  bool display_;
  Vec_Diagnostic diagnostics_;
} DiagnosticManager;

DiagnosticManager DiagnosticManager_new (const SourceFile *);
void DiagnosticManager_drop (DiagnosticManager *self);
size_t DiagnosticManager_num_total (const DiagnosticManager *self);
void DiagnosticManager_set_colored (DiagnosticManager *self, bool colored);
void DiagnosticManager_set_display (DiagnosticManager *self, bool display);
void DiagnosticManager_diagnose_invalid_token (DiagnosticManager *self,
                                               Span span);
void DiagnosticManager_diagnose_expected_tokens_or_nodes (
    DiagnosticManager *self, Span span, Array_TokenKind tokens,
    Array_ASTKind nodes);
void DiagnosticManager_diagnose_expected_token (DiagnosticManager *self,
                                                Span span,
                                                enum TokenKind token);
void DiagnosticManager_diagnose_expected_node (DiagnosticManager *self,
                                               Span span, enum ASTKind node);
void DiagnosticManager_diagnose_unexpected_token (DiagnosticManager *self,
                                                  Span span);
void DiagnosticManager_diagnose_invalid_type (DiagnosticManager *self,
                                              Span span);

#endif
