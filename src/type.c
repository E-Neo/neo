/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "type.h"

#include <assert.h>
#include <stdbool.h>

#include "string.h"
#include "vec_macro.h"

NEO_IMPL_VEC (TypeId, TypeId)
NEO_IMPL_VEC (Type, Type)

static TypeManager
TypeManager_new_base ()
{
  Vec_Type types = Vec_Type_new ();
#define NEO_TYPEKIND(NAME, UNUSED)                                            \
  Vec_Type_push (&types, (Type){ .kind_ = TYPE_##NAME });
#include "type_kind.def"
#undef NEO_TYPEKIND
  return (TypeManager){ .types_ = types };
}

TypeManager
TypeManager_new ()
{
  TypeManager type_mgr = TypeManager_new_base ();
  TypeManager_push_union (&type_mgr, TypeManager_get_false (&type_mgr),
                          TypeManager_get_true (&type_mgr));
  return type_mgr;
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
#define NEO_TYPEKIND(NAME, L)                                                 \
  case TYPE_##NAME:                                                           \
    {                                                                         \
      return String_from_cstring (#L);                                        \
    }
#include "type_kind.def"
#undef NEO_TYPEKIND
    default:
      {
        return String_from_cstring ("Advanced type");
      }
    }
}

bool
TypeManager_is_invalid (const TypeManager *self, TypeId id)
{
  assert (id < Vec_Type_len (&self->types_));
  return TypeManager_get_type (self, id)->kind_ == TYPE_INVALID;
}

bool
TypeManager_is_union (const TypeManager *self, TypeId id)
{
  assert (id < Vec_Type_len (&self->types_));
  return TypeManager_get_type (self, id)->kind_ == TYPE_UNION;
}

bool
TypeManager_is_subtype_of (const TypeManager *self, TypeId left, TypeId right)
{
  assert (left < Vec_Type_len (&self->types_));
  assert (right < Vec_Type_len (&self->types_));
  if (left == right)
    {
      return true;
    }
  if (TypeManager_is_union (self, right))
    {
      const Type *right_type = TypeManager_get_type (self, right);
      return TypeManager_is_subtype_of (self, left, right_type->union_.left_)
             || TypeManager_is_subtype_of (self, left,
                                           right_type->union_.right_);
    }
  return false;
}

TypeId
TypeManager_get_invalid (const TypeManager *self)
{
  (void)self;
  return TYPE_INVALID;
}

TypeId
TypeManager_get_void (const TypeManager *self)
{
  (void)self;
  return TYPE_VOID;
}

TypeId
TypeManager_get_false (const TypeManager *self)
{
  (void)self;
  return TYPE_FALSE;
}

TypeId
TypeManager_get_true (const TypeManager *self)
{
  (void)self;
  return TYPE_TRUE;
}

TypeId
TypeManager_get_bool (const TypeManager *self)
{
  (void)self;
  for (const Type *type = Vec_Type_cbegin (&self->types_);
       type < Vec_Type_cend (&self->types_); type++)
    {
      if (type->kind_ == TYPE_UNION
          && (type->union_.left_ == TypeManager_get_false (self)
              && type->union_.right_ == TypeManager_get_true (self)))
        {
          return type - Vec_Type_cbegin (&self->types_);
        }
    }
  assert (false);
  return TypeManager_get_invalid (self);
}

static TypeId
TypeManager_get_next_id (const TypeManager *self)
{
  return Vec_Type_len (&self->types_);
}

TypeId
TypeManager_push_union (TypeManager *self, TypeId t1, TypeId t2)
{
  if (t1 == t2)
    {
      return t1;
    }
  TypeId id = TypeManager_get_next_id (self);
  TypeId left, right;
  if (t1 < t2)
    {
      left = t1;
      right = t2;
    }
  else
    {
      left = t2;
      right = t1;
    }
  Vec_Type_push (
      &self->types_,
      (Type){ .kind_ = TYPE_UNION,
              .union_ = (TypeUnion){ .left_ = left, .right_ = right } });
  return id;
}

#ifdef TESTS
#include "test.h"

NEO_TEST (test_is_subtype_of_00)
{
  TypeManager type_mgr = TypeManager_new ();
  ASSERT_U64_EQ (TypeManager_is_subtype_of (&type_mgr,
                                            TypeManager_get_void (&type_mgr),
                                            TypeManager_get_bool (&type_mgr)),
                 false);
  ASSERT_U64_EQ (TypeManager_is_subtype_of (&type_mgr,
                                            TypeManager_get_false (&type_mgr),
                                            TypeManager_get_bool (&type_mgr)),
                 true);
  ASSERT_U64_EQ (TypeManager_is_subtype_of (&type_mgr,
                                            TypeManager_get_true (&type_mgr),
                                            TypeManager_get_bool (&type_mgr)),
                 true);
  TypeManager_drop (&type_mgr);
}

NEO_TESTS (type_tests, test_is_subtype_of_00)

#endif
