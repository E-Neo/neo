/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include <stddef.h>

typedef struct Position
{
  size_t line_;   /* 1-based.  */
  size_t column_; /* 0-based.  */
} Position;

Position Position_new (size_t line, size_t column);
size_t Position_get_line (const Position *self);
size_t Position_get_column (const Position *self);

#ifndef NEO_FILE_POSITION_H_
#define NEO_FILE_POSITION_H_
#endif
