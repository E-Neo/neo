/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_DIAGNOSTIC_MANAGER_H_
#define NEO_DIAGNOSTIC_MANAGER_H_

#include <stdbool.h>

#include "core/macro/vec.h"
#include "core/string.h"
#include "file/span.h"

enum DiagnosticLevel
{
  DIAGNOSTIC_LEVEL_ERROR,
  DIAGNOSTIC_LEVEL_NOTE
};

typedef struct Diagnostic Diagnostic;

NEO_DECL_VEC (Diagnostic, Diagnostic)

typedef struct Diagnostic
{
  enum DiagnosticLevel level_;
  String message_;
  const char *content_cbegin_;
  Span span_;
  String span_message_;
  Vec_Diagnostic sub_diags_;
} Diagnostic;

typedef struct DiagnosticManager
{
  Vec_Diagnostic diags_;
  bool color_;
} DiagnosticManager;

DiagnosticManager DiagnosticManager_new ();
void DiagnosticManager_drop (DiagnosticManager *self);

#endif
