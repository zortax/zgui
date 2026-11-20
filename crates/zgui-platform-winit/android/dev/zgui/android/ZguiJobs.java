package dev.zgui.android;

import android.app.job.JobParameters;
import android.app.job.JobService;

/** Runs scheduled jobs through the Rust `android_background!` handler on a worker thread. */
public final class ZguiJobs extends JobService {
    @Override public boolean onStartJob(JobParameters params) {
        Zgui.init(this);
        Zgui.io.execute(() -> {
            try { Zgui.dispatch(this, "job", Integer.toString(params.getJobId())); }
            finally { jobFinished(params, false); }
        });
        return true;
    }

    @Override public boolean onStopJob(JobParameters params) { return false; }
}
