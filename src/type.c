/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "type.h"

#include <assert.h>

#include "vec_macro.h"

NEO_IMPL_VEC (TypeId, TypeId)
NEO_IMPL_VEC (Type, Type)

TypeManager
TypeManager_new ()
{
  Vec_Type types = Vec_Type_new ();
#define NEO_TYPEKIND(NAME, UNUSED)                                            \
  Vec_Type_push (&types, (Type){ .kind_ = TYPE_##NAME });
#include "type_kind.def"
#undef NEO_TYPEKIND
  return (TypeManager){ .types_ = types };
}

void
TypeManager_drop (TypeManager *self)
{
  Vec_Type_drop (&self->types_);
}

const Type *
TypeManager_get_type (const TypeManager *self, TypeId id)
{
  assert (id < Vec_Type_len (&self->types_));
  return Vec_Type_cbegin (&self->types_) + id;
}
