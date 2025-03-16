# Environment Variables Implementation

This document records the implementation of environment variables in the xv6rs operating system.

## Implementation Plan

1. Kernel-side Changes
   - Add data structures for environment variables to the process structure
   - Implement system calls
   - Extend fork processing

2. User-space Changes
   - Add system call interfaces
   - Add assembly stubs
   - Create utility commands

## Implementation Details

### Data Structure

Environment variables are stored in the `ProcData` structure as follows:

```rust
pub struct ProcData {
    // ... other fields ...
    pub env_vars: Option<BTreeMap<String, String>>,
}
```

- `env_vars` is an `Option<BTreeMap<String, String>>` that maps environment variable names to their values
- It is initialized as `None` in `ProcData::new()` and set to `Some(BTreeMap::new())` in `ProcData::init_context()`
- When a process is forked, environment variables are copied from parent to child
- When a process is freed, environment variables are cleared

### System Calls

Four system calls have been implemented for environment variable management:

1. `getenv` (syscall number 28)
   - Arguments: 
     - `name`: Pointer to null-terminated string containing the variable name
     - `value`: Pointer to buffer where the value will be stored
     - `size`: Size of the buffer
   - Returns: Length of the value, or -1 if the variable doesn't exist
   - Behavior: Copies the value of the environment variable to the provided buffer

2. `setenv` (syscall number 29)
   - Arguments:
     - `name`: Pointer to null-terminated string containing the variable name
     - `value`: Pointer to null-terminated string containing the variable value
     - `overwrite`: If non-zero, overwrite existing variable; if zero, don't overwrite
   - Returns: 0 on success, -1 on error
   - Behavior: Sets the value of an environment variable

3. `unsetenv` (syscall number 30)
   - Arguments:
     - `name`: Pointer to null-terminated string containing the variable name
   - Returns: 0 on success, -1 if the variable doesn't exist
   - Behavior: Removes an environment variable

4. `listenv` (syscall number 31)
   - Arguments:
     - `buf`: Pointer to buffer where the list will be stored
     - `size`: Size of the buffer
   - Returns: Number of bytes written to the buffer, or -1 on error
   - Behavior: Lists all environment variables in the format "name=value\0name=value\0..."

### Implementation Notes

- Environment variables are stored in kernel memory, not user memory
- The `fork()` system call copies environment variables from parent to child
- The `exec()` system call preserves environment variables
- Environment variables are cleared when a process exits

## Implementation Steps

### Step 1: Create Documentation File ✓

- Create this file (`docs/environment_variables_implementation.md`)

### Step 2: Extend Process Structure ✓

- Add data structure for storing environment variables to `ProcData` struct in `kernel/src/proc.rs`
- Update `ProcData::new()` method to initialize `env_vars`

### Step 3: Add System Calls (Kernel-side) ✓

- Add new methods to the `Syscall` trait in `kernel/src/proc/syscall.rs`
  - `sys_getenv`
  - `sys_setenv`
  - `sys_unsetenv`
  - `sys_listenv`
- Implement these methods
- Add new system call numbers and method calls to the `match` statement in `Proc::syscall()`

### Step 4: Extend Fork Processing ✓

- Extend the `Proc::fork()` method to copy environment variables from parent to child process

### Step 5: Add User-space System Call Interfaces

- Add functions for new system calls to `user/src/syscall.rs`

### Step 6: Add Assembly Stubs

- Add assembly stubs for new system calls to `user/src/usys.S`

### Step 7: Create Utility Commands

- `user/src/bin/env.rs` - Command to display all environment variables
- `user/src/bin/export.rs` - Command to set environment variables
- `user/src/bin/unset.rs` - Command to delete environment variables

### Step 8: Testing

- Test basic environment variable setting, getting, and deleting
- Test environment variable inheritance during fork
