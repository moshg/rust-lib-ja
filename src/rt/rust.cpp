/**
 * Main entry point into the Rust runtime. Here we initialize the kernel,
 * create the initial scheduler and run the main task.
 */

#include "rust_globals.h"
#include "rust_kernel.h"
#include "rust_util.h"
#include "rust_scheduler.h"
#include "rust_gc_metadata.h"

// Creates a rust argument vector from the platform argument vector
struct
command_line_args : public kernel_owned<command_line_args>
{
    rust_kernel *kernel;
    rust_task *task;
    int argc;
    char **argv;

    // [str] passed to rust_task::start.
    rust_vec_box *args;

    command_line_args(rust_task *task,
                      int sys_argc,
                      char **sys_argv)
        : kernel(task->kernel),
          task(task),
          argc(sys_argc),
          argv(sys_argv)
    {
#if defined(__WIN32__)
        LPCWSTR cmdline = GetCommandLineW();
        LPWSTR *wargv = CommandLineToArgvW(cmdline, &argc);
        kernel->win32_require("CommandLineToArgvW", wargv != NULL);
        argv = (char **) kernel->malloc(sizeof(char*) * argc,
                                        "win32 command line");
        for (int i = 0; i < argc; ++i) {
            int n_chars = WideCharToMultiByte(CP_UTF8, 0, wargv[i], -1,
                                              NULL, 0, NULL, NULL);
            kernel->win32_require("WideCharToMultiByte(0)", n_chars != 0);
            argv[i] = (char *) kernel->malloc(n_chars,
                                              "win32 command line arg");
            n_chars = WideCharToMultiByte(CP_UTF8, 0, wargv[i], -1,
                                          argv[i], n_chars, NULL, NULL);
            kernel->win32_require("WideCharToMultiByte(1)", n_chars != 0);
        }
        LocalFree(wargv);
#endif

        args = make_str_vec(kernel, argc, argv);
    }

    ~command_line_args() {
        for (int i = 0; i < argc; ++i) {
            rust_vec *s = ((rust_vec**)&args->body.data)[i];
            kernel->free(s);
        }
        kernel->free(args);

#ifdef __WIN32__
        for (int i = 0; i < argc; ++i) {
            kernel->free(argv[i]);
        }
        kernel->free(argv);
#endif
    }
};

void* global_crate_map = NULL;

/**
   The runtime entrypoint. The (C ABI) main function generated by rustc calls
   `rust_start`, providing the address of the Rust ABI main function, the
   platform argument vector, and a `crate_map` the provides some logging
   metadata.
*/
extern "C" CDECL int
rust_start(uintptr_t main_fn, int argc, char **argv, void* crate_map) {

    // Load runtime configuration options from the environment.
    // FIXME #1497: Should provide a way to get these from the command
    // line as well.
    rust_env *env = load_env(argc, argv);

    global_crate_map = crate_map;

    update_gc_metadata(crate_map);

    update_log_settings(crate_map, env->logspec);

    rust_kernel *kernel = new rust_kernel(env);

    // Create the main scheduler and the main task
    rust_sched_id sched_id = kernel->create_scheduler(env->num_sched_threads);
    rust_scheduler *sched = kernel->get_scheduler_by_id(sched_id);
    assert(sched != NULL);
    rust_task *root_task = sched->create_task(NULL, "main");

    // Build the command line arguments to pass to the root task
    command_line_args *args
        = new (kernel, "main command line args")
        command_line_args(root_task, argc, argv);

    LOG(root_task, dom, "startup: %d args in 0x%" PRIxPTR,
        args->argc, (uintptr_t)args->args);
    for (int i = 0; i < args->argc; i++) {
        LOG(root_task, dom, "startup: arg[%d] = '%s'", i, args->argv[i]);
    }

    // Schedule the main Rust task
    root_task->start((spawn_fn)main_fn, NULL, args->args);

    // At this point the task lifecycle is responsible for it
    // and our pointer may not be valid
    root_task = NULL;

    // Run the kernel until all schedulers exit
    int ret = kernel->run();

    delete args;
    delete kernel;
    free_env(env);

    return ret;
}

//
// Local Variables:
// mode: C++
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
//
