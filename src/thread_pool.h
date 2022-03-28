/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_THREAD_POOL_H
#define NEO_THREAD_POOL_H

#include <stddef.h>
#include <threads.h>

#include "queue_macro.h"
#include "vec.h"
#include "vec_macro.h"

typedef struct ThreadPoolJob
{
  void (*job_) (void *);
  void *job_arg_;
} ThreadPoolJob;

NEO_DECL_VEC (ThreadPoolJob, ThreadPoolJob)
NEO_DECL_QUEUE (ThreadPoolJob, ThreadPoolJob)

typedef struct ThreadPoolData
{
  Queue_ThreadPoolJob jobs_;
  Vec_thrd_t threads_;
  mtx_t lock_;
  cnd_t new_job_notify_;
  size_t num_running_;
} ThreadPoolData;

typedef struct ThreadPool
{
  size_t num_threads_;
  ThreadPoolData *data_;
} ThreadPool;

ThreadPool ThreadPool_new (size_t num_threads);
void ThreadPool_drop (ThreadPool *self);
void ThreadPool_execute (ThreadPool *self, void (*job) (void *),
                         void *job_arg);

#endif
