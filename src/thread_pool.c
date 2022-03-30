/* Copyright (C) 2022 Yanxuan Cui <e-neo@qq.com>, all rights reserved.  */

#include "thread_pool.h"

#include <stdbool.h>
#include <stddef.h>
#include <stdlib.h>
#include <threads.h>

#include "queue_macro.h"
#include "vec.h"
#include "vec_macro.h"

NEO_IMPL_VEC (ThreadPoolJob, ThreadPoolJob)
NEO_IMPL_QUEUE (ThreadPoolJob, ThreadPoolJob)

static void
ThreadPoolData_lock (ThreadPoolData *self)
{
  if (mtx_lock (&self->lock_) != thrd_success)
    {
      abort ();
    }
}

static void
ThreadPoolData_unlock (ThreadPoolData *self)
{
  if (mtx_unlock (&self->lock_) != thrd_success)
    {
      abort ();
    }
}

static void
ThreadPoolData_wait_new_job_notify (ThreadPoolData *self)
{
  if (cnd_wait (&self->new_job_notify_, &self->lock_) != thrd_success)
    {
      abort ();
    }
}

static void
ThreadPoolData_broadcast_new_job_notify (ThreadPoolData *self)
{
  if (cnd_broadcast (&self->new_job_notify_) != thrd_success)
    {
      abort ();
    }
}

static void
exit_work (void *pool_data)
{
  (void)pool_data;
}

static int
do_work (void *pool_data)
{
  ThreadPoolData *data = pool_data;
  while (true)
    {
      ThreadPoolData_lock (data);
      while (Queue_ThreadPoolJob_is_empty (&data->jobs_))
        {
          ThreadPoolData_wait_new_job_notify (data);
        }
      ThreadPoolJob job = Queue_ThreadPoolJob_pop_front (&data->jobs_);
      ThreadPoolData_unlock (data);
      job.job_ (job.job_arg_);
      if (job.job_ == exit_work)
        {
          break;
        }
    }
  return 0;
}

ThreadPool
ThreadPool_new (size_t num_threads)
{
  ThreadPoolData *data = (ThreadPoolData *)malloc (sizeof (ThreadPoolData));
  if (data == NULL)
    {
      abort ();
    }
  data->jobs_ = Queue_ThreadPoolJob_new ();
  data->threads_ = Vec_thrd_t_with_capacity (num_threads);
  if (mtx_init (&data->lock_, mtx_plain) != thrd_success
      || cnd_init (&data->new_job_notify_) != thrd_success)
    {
      abort ();
    }
  for (size_t i = 0; i < num_threads; i++)
    {
      Vec_thrd_t_push_uninit (&data->threads_);
      if (thrd_create (Vec_thrd_t_begin (&data->threads_) + i, do_work, data)
          != thrd_success)
        {
          abort ();
        }
      ThreadPoolData_lock (data);
      ThreadPoolData_unlock (data);
    }
  return (ThreadPool){ .num_threads_ = num_threads, .data_ = data };
}

void
ThreadPool_drop (ThreadPool *self)
{
  for (size_t i = 0; i < self->num_threads_; i++)
    {
      ThreadPool_execute (self, exit_work, self->data_);
    }
  for (const thrd_t *thread = Vec_thrd_t_cbegin (&self->data_->threads_);
       thread < Vec_thrd_t_cend (&self->data_->threads_); thread++)
    {
      if (thrd_join (*thread, NULL) != thrd_success)
        {
          abort ();
        }
    }
  mtx_destroy (&self->data_->lock_);
  cnd_destroy (&self->data_->new_job_notify_);
  Queue_ThreadPoolJob_drop (&self->data_->jobs_);
  Vec_thrd_t_drop (&self->data_->threads_);
  free (self->data_);
  self->data_ = NULL;
}

void
ThreadPool_execute (ThreadPool *self, void (*job) (void *), void *job_arg)
{
  ThreadPoolData_lock (self->data_);
  Queue_ThreadPoolJob_push_back (
      &self->data_->jobs_,
      (ThreadPoolJob){ .job_ = job, .job_arg_ = job_arg });
  ThreadPoolData_broadcast_new_job_notify (self->data_);
  ThreadPoolData_unlock (self->data_);
}
