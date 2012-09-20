/*!

Task local data management

Allows storing boxes with arbitrary types inside, to be accessed
anywhere within a task, keyed by a pointer to a global finaliser
function. Useful for dynamic variables, singletons, and interfacing
with foreign code with bad callback interfaces.

To use, declare a monomorphic global function at the type to store,
and use it as the 'key' when accessing. See the 'tls' tests below for
examples.

Casting 'Arcane Sight' reveals an overwhelming aura of Transmutation
magic.

*/

export LocalDataKey;
export local_data_pop;
export local_data_get;
export local_data_set;
export local_data_modify;

use local_data_priv::{
    local_pop,
    local_get,
    local_set,
    local_modify
};

/**
 * Indexes a task-local data slot. The function's code pointer is used for
 * comparison. Recommended use is to write an empty function for each desired
 * task-local data slot (and use class destructors, not code inside the
 * function, if specific teardown is needed). DO NOT use multiple
 * instantiations of a single polymorphic function to index data of different
 * types; arbitrary type coercion is possible this way.
 *
 * One other exception is that this global state can be used in a destructor
 * context to create a circular @-box reference, which will crash during task
 * failure (see issue #3039).
 *
 * These two cases aside, the interface is safe.
 */
type LocalDataKey<T: Owned> = &fn(+@T);

/**
 * Remove a task-local data value from the table, returning the
 * reference that was originally created to insert it.
 */
unsafe fn local_data_pop<T: Owned>(
    key: LocalDataKey<T>) -> Option<@T> {

    local_pop(rustrt::rust_get_task(), key)
}
/**
 * Retrieve a task-local data value. It will also be kept alive in the
 * table until explicitly removed.
 */
unsafe fn local_data_get<T: Owned>(
    key: LocalDataKey<T>) -> Option<@T> {

    local_get(rustrt::rust_get_task(), key)
}
/**
 * Store a value in task-local data. If this key already has a value,
 * that value is overwritten (and its destructor is run).
 */
unsafe fn local_data_set<T: Owned>(
    key: LocalDataKey<T>, +data: @T) {

    local_set(rustrt::rust_get_task(), key, data)
}
/**
 * Modify a task-local data value. If the function returns 'None', the
 * data is removed (and its reference dropped).
 */
unsafe fn local_data_modify<T: Owned>(
    key: LocalDataKey<T>,
    modify_fn: fn(Option<@T>) -> Option<@T>) {

    local_modify(rustrt::rust_get_task(), key, modify_fn)
}

#[test]
fn test_tls_multitask() unsafe {
    fn my_key(+_x: @~str) { }
    local_data_set(my_key, @~"parent data");
    do task::spawn unsafe {
        assert local_data_get(my_key).is_none(); // TLS shouldn't carry over.
        local_data_set(my_key, @~"child data");
        assert *(local_data_get(my_key).get()) == ~"child data";
        // should be cleaned up for us
    }
    // Must work multiple times
    assert *(local_data_get(my_key).get()) == ~"parent data";
    assert *(local_data_get(my_key).get()) == ~"parent data";
    assert *(local_data_get(my_key).get()) == ~"parent data";
}

#[test]
fn test_tls_overwrite() unsafe {
    fn my_key(+_x: @~str) { }
    local_data_set(my_key, @~"first data");
    local_data_set(my_key, @~"next data"); // Shouldn't leak.
    assert *(local_data_get(my_key).get()) == ~"next data";
}

#[test]
fn test_tls_pop() unsafe {
    fn my_key(+_x: @~str) { }
    local_data_set(my_key, @~"weasel");
    assert *(local_data_pop(my_key).get()) == ~"weasel";
    // Pop must remove the data from the map.
    assert local_data_pop(my_key).is_none();
}

#[test]
fn test_tls_modify() unsafe {
    fn my_key(+_x: @~str) { }
    local_data_modify(my_key, |data| {
        match data {
            Some(@val) => fail ~"unwelcome value: " + val,
            None       => Some(@~"first data")
        }
    });
    local_data_modify(my_key, |data| {
        match data {
            Some(@~"first data") => Some(@~"next data"),
            Some(@val)           => fail ~"wrong value: " + val,
            None                 => fail ~"missing value"
        }
    });
    assert *(local_data_pop(my_key).get()) == ~"next data";
}

#[test]
fn test_tls_crust_automorestack_memorial_bug() unsafe {
    // This might result in a stack-canary clobber if the runtime fails to set
    // sp_limit to 0 when calling the cleanup extern - it might automatically
    // jump over to the rust stack, which causes next_c_sp to get recorded as
    // Something within a rust stack segment. Then a subsequent upcall (esp.
    // for logging, think vsnprintf) would run on a stack smaller than 1 MB.
    fn my_key(+_x: @~str) { }
    do task::spawn {
        unsafe { local_data_set(my_key, @~"hax"); }
    }
}

#[test]
fn test_tls_multiple_types() unsafe {
    fn str_key(+_x: @~str) { }
    fn box_key(+_x: @@()) { }
    fn int_key(+_x: @int) { }
    do task::spawn unsafe {
        local_data_set(str_key, @~"string data");
        local_data_set(box_key, @@());
        local_data_set(int_key, @42);
    }
}

#[test]
fn test_tls_overwrite_multiple_types() {
    fn str_key(+_x: @~str) { }
    fn box_key(+_x: @@()) { }
    fn int_key(+_x: @int) { }
    do task::spawn unsafe {
        local_data_set(str_key, @~"string data");
        local_data_set(int_key, @42);
        // This could cause a segfault if overwriting-destruction is done with
        // the crazy polymorphic transmute rather than the provided finaliser.
        local_data_set(int_key, @31337);
    }
}

#[test]
#[should_fail]
#[ignore(cfg(windows))]
fn test_tls_cleanup_on_failure() unsafe {
    fn str_key(+_x: @~str) { }
    fn box_key(+_x: @@()) { }
    fn int_key(+_x: @int) { }
    local_data_set(str_key, @~"parent data");
    local_data_set(box_key, @@());
    do task::spawn unsafe { // spawn_linked
        local_data_set(str_key, @~"string data");
        local_data_set(box_key, @@());
        local_data_set(int_key, @42);
        fail;
    }
    // Not quite nondeterministic.
    local_data_set(int_key, @31337);
    fail;
}
