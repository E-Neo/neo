/* Copyright (c) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#ifndef NEO_CORE_THREAD_POOL_H_
#define NEO_CORE_THREAD_POOL_H_

#include <stddef.h>
#include <threads.h>

#include "core/macro/queue.h"
#include "core/vec.h"

typedef struct ThreadPoolJob
{
  void (*job_) (void *);
  void *job_arg_;
} ThreadPoolJob;

NEO_DECL_QUEUE (ThreadPoolJob, ThreadPoolJob)

typedef struct ThreadPoolData
{
  Queue_ThreadPoolJob jobs_;
  Vec_thrd_t threads_;
  mtx_t lock_;
  cnd_t new_job_notify_;
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
