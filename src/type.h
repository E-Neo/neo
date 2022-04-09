/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_TYPE_H
#define NEO_TYPE_H

#include <stdint.h>

#include "vec_macro.h"

typedef uint32_t TypeId;
typedef struct Type Type;

NEO_DECL_VEC (TypeId, TypeId)
NEO_DECL_VEC (Type, Type)

enum TypeKind
{
#define NEO_TYPEKIND(NAME, UNUSED) TYPE_##NAME,
#include "type_kind.def"
#undef NEO_TYPEKIND
};

typedef struct Type
{
  enum TypeKind kind_;
} Type;

typedef struct TypeManager
{
  Vec_Type types_;
} TypeManager;

TypeManager TypeManager_new ();
void TypeManager_drop (TypeManager *self);
const Type *TypeManager_get_type (const TypeManager *self, TypeId id);

#endif
