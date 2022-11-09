/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "diagnostic/manager.h"

#include <stdbool.h>

#include "core/macro/vec.h"

NEO_IMPL_VEC (Diagnostic, Diagnostic)

DiagnosticManager
DiagnosticManager_new ()
{
  return (DiagnosticManager){ .diags_ = Vec_Diagnostic_new (),
                              .color_ = true };
}

void
DiagnosticManager_drop (DiagnosticManager *self)
{
  Vec_Diagnostic_drop (&self->diags_);
}
