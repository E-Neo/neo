/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_FILE_TEXT_MANAGER_H_
#define NEO_FILE_TEXT_MANAGER_H_

#include "core/string.h"

#include <stddef.h>

#include "core/vec.h"
#include "file/position.h"

typedef struct TextManager
{
  String path_;
  String content_;
  Vec_size_t lines_;
} TextManager;

TextManager TextManager_new (String path, String content);
void TextManager_drop (TextManager *self);
const String *TextManager_get_path (const TextManager *self);
const String *TextManager_get_content (const TextManager *self);
Position TextManager_lookup_position (const TextManager *self, size_t offset);

#endif
