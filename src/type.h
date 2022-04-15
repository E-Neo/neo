/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_TYPE_H
#define NEO_TYPE_H

#include <stdint.h>

#include "string.h"
#include "vec_macro.h"

typedef uint32_t TypeId;

NEO_DECL_VEC (TypeId, TypeId)

enum TypeKind
{
#define NEO_TYPEKIND(N, UNUSED) TYPE_##N,
#include "type_kind.def"
#undef NEO_TYPEKIND
};

typedef struct Type
{
  enum TypeKind kind_;
} Type;

NEO_DECL_VEC (Type, Type)

typedef struct TypeManager
{
  Vec_Type types_;
} TypeManager;

TypeManager TypeManager_new ();
void TypeManager_drop (TypeManager *self);
const Type *TypeManager_get_type (const TypeManager *self, TypeId id);
String TypeManager_to_string (const TypeManager *self, TypeId id);
bool TypeManager_is_unknown (const TypeManager *self, TypeId id);
bool TypeManager_is_invalid (const TypeManager *self, TypeId id);
bool TypeManager_is_bool (const TypeManager *self, TypeId id);
bool TypeManager_are_equal (const TypeManager *self, TypeId x, TypeId y);
TypeId TypeManager_get_invalid (const TypeManager *self);
TypeId TypeManager_get_bool (const TypeManager *self);

#endif
