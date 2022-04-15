/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "type.h"

#include <assert.h>
#include <stdbool.h>

#include "string.h"
#include "vec_macro.h"

NEO_IMPL_VEC (TypeId, TypeId)
NEO_IMPL_VEC (Type, Type)

TypeManager
TypeManager_new ()
{
  Vec_Type types = Vec_Type_new ();
#define NEO_TYPEKIND(N, UNUSED)                                               \
  Vec_Type_push (&types, (Type){ .kind_ = TYPE_##N });
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

String
TypeManager_to_string (const TypeManager *self, TypeId id)
{
  switch (TypeManager_get_type (self, id)->kind_)
    {
#define NEO_TYPEKIND(N, L)                                                    \
  case TYPE_##N:                                                              \
    {                                                                         \
      return String_from_cstring (L);                                         \
    }
#include "type_kind.def"
#undef NEO_TYPEKIND
    default:
      {
        return String_from_cstring ("internal error");
      }
    }
}

bool
TypeManager_is_unknown (const TypeManager *self, TypeId id)
{
  return TypeManager_get_type (self, id)->kind_ == TYPE_UNKNOWN;
}

bool
TypeManager_is_invalid (const TypeManager *self, TypeId id)
{
  return TypeManager_get_type (self, id)->kind_ == TYPE_INVALID;
}

bool
TypeManager_is_bool (const TypeManager *self, TypeId id)
{
  return TypeManager_get_type (self, id)->kind_ == TYPE_BOOL;
}

bool
TypeManager_are_equal (const TypeManager *self, TypeId x, TypeId y)
{
  return TypeManager_get_type (self, x) == TypeManager_get_type (self, y);
}

TypeId
TypeManager_get_invalid (const TypeManager *self)
{
  assert (TYPE_INVALID < Vec_Type_len (&self->types_));
  assert (Vec_Type_cbegin (&self->types_)[TYPE_INVALID].kind_ == TYPE_INVALID);
  return TYPE_INVALID;
}

TypeId
TypeManager_get_bool (const TypeManager *self)
{
  assert (TYPE_BOOL < Vec_Type_len (&self->types_));
  assert (Vec_Type_cbegin (&self->types_)[TYPE_BOOL].kind_ == TYPE_BOOL);
  return TYPE_BOOL;
}
