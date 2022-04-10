/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_TYPE_H
#define NEO_TYPE_H

#include <stdbool.h>
#include <stdint.h>

#include "string.h"
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
  TYPE_UNION
};

typedef struct TypeUnion
{
  TypeId left_;
  TypeId right_;
} TypeUnion;

typedef struct Type
{
  enum TypeKind kind_;
  union
  {
    TypeUnion union_;
  };
} Type;

typedef struct TypeManager
{
  Vec_Type types_;
} TypeManager;

TypeManager TypeManager_new ();
void TypeManager_drop (TypeManager *self);
const Type *TypeManager_get_type (const TypeManager *self, TypeId id);
String TypeManager_to_string (const TypeManager *self, TypeId id);
bool TypeManager_is_invalid (const TypeManager *self, TypeId id);
bool TypeManager_is_union (const TypeManager *self, TypeId id);
bool TypeManager_is_subtype_of (const TypeManager *self, TypeId left,
                                TypeId right);
TypeId TypeManager_get_invalid (const TypeManager *self);
TypeId TypeManager_get_void (const TypeManager *self);
TypeId TypeManager_get_false (const TypeManager *self);
TypeId TypeManager_get_true (const TypeManager *self);
TypeId TypeManager_get_bool (const TypeManager *self);
TypeId TypeManager_push_union (TypeManager *self, TypeId t1, TypeId t2);

#ifdef TESTS
#include "test.h"
Tests type_tests ();
#endif

#endif
