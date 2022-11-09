/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "file/position.h"

Position
Position_new (size_t line, size_t column)
{
  return (Position){ .line_ = line, .column_ = column };
}

size_t
Position_get_line (const Position *self)
{
  return self->line_;
}

size_t
Position_get_column (const Position *self)
{
  return self->column_;
}
