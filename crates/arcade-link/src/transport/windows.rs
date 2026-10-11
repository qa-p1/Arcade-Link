//! Blocking event waits over overlapped named-pipe I/O. No polling or worker
//! timers: None is an infinite wait; a deadline uses the OS wait timeout.

use interprocess::local_socket::Stream;
use std::{
    io,
    os::windows::io::{AsHandle, AsRawHandle},
    sync::Mutex,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, ERROR_BROKEN_PIPE, ERROR_HANDLE_EOF, ERROR_IO_PENDING, ERROR_OPERATION_ABORTED, HANDLE, WAIT_FAILED, WAIT_OBJECT_0,
        WAIT_TIMEOUT,
    },
    Storage::FileSystem::{ReadFile, WriteFile},
    System::{
        Threading::{CreateEventW, SetEvent, WaitForMultipleObjects, WaitForSingleObject, INFINITE},
        IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
    },
};

struct Event(HANDLE);
// SAFETY: Windows event handles support concurrent wait/set operations. The
// owning Arc outlives all operations; Drop alone closes the handle.
unsafe impl Send for Event {}
unsafe impl Sync for Event {}
impl Event {
    fn new() -> io::Result<Self> {
        let handle = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if handle.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(handle))
        }
    }
}
impl Drop for Event {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub(super) struct State {
    cancel: Event,
    recv: Mutex<Option<Duration>>,
    send: Mutex<Option<Duration>>,
}
impl State {
    pub(super) fn new() -> io::Result<Self> {
        Ok(Self { cancel: Event::new()?, recv: Mutex::new(None), send: Mutex::new(None) })
    }
    pub(super) fn recv_timeout(&self, value: Option<Duration>) {
        *self.recv.lock().unwrap_or_else(|e| e.into_inner()) = value;
    }
    pub(super) fn send_timeout(&self, value: Option<Duration>) {
        *self.send.lock().unwrap_or_else(|e| e.into_inner()) = value;
    }
    pub(super) fn cancel(&self) {
        unsafe {
            SetEvent(self.cancel.0);
        }
    }
    pub(super) fn read(&self, stream: &Stream, buf: &mut [u8]) -> io::Result<usize> {
        // SAFETY: the buffer remains borrowed until the operation completes,
        // including cancellation completion, and Windows writes at most len.
        unsafe { self.transfer(stream, buf.as_mut_ptr(), buf.len(), false) }
    }
    pub(super) fn write(&self, stream: &Stream, buf: &[u8]) -> io::Result<usize> {
        // SAFETY: WriteFile only reads this buffer, kept live until completion.
        unsafe { self.transfer(stream, buf.as_ptr().cast_mut(), buf.len(), true) }
    }
    unsafe fn transfer(&self, stream: &Stream, buffer: *mut u8, length: usize, sending: bool) -> io::Result<usize> {
        if unsafe { WaitForSingleObject(self.cancel.0, 0) } == WAIT_OBJECT_0 {
            return Err(io::Error::new(io::ErrorKind::ConnectionAborted, "connection closed"));
        }
        let Stream::NamedPipe(pipe) = stream;
        let handle = pipe.inner().as_handle().as_raw_handle();
        let event = Event::new()?;
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        overlapped.hEvent = event.0;
        let mut bytes = 0;
        let length = u32::try_from(length).unwrap_or(u32::MAX);
        let ready = if sending {
            // Preserve buffered bytes through interprocess's drop-time drain.
            pipe.inner().mark_dirty();
            unsafe { WriteFile(handle, buffer.cast_const(), length, &mut bytes, &mut overlapped) }
        } else {
            unsafe { ReadFile(handle, buffer, length, &mut bytes, &mut overlapped) }
        };
        if ready == 0 {
            let error = unsafe { GetLastError() };
            if !sending && [ERROR_BROKEN_PIPE, ERROR_HANDLE_EOF].contains(&error) {
                return Ok(0);
            }
            if error != ERROR_IO_PENDING {
                return Err(io::Error::from_raw_os_error(error as i32));
            }
        }
        let mut interrupted = None;
        if ready == 0 {
            let timeout = *if sending { &self.send } else { &self.recv }.lock().unwrap_or_else(|e| e.into_inner());
            // Round up positive sub-millisecond timeouts; reserve INFINITE.
            let millis = timeout
                .map_or(INFINITE, |d| u32::try_from(d.as_millis() + u128::from(d.subsec_nanos() % 1_000_000 != 0)).unwrap_or(INFINITE - 1).min(INFINITE - 1));
            let handles = [self.cancel.0, event.0];
            match unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, millis) } {
                n if n == WAIT_OBJECT_0 + 1 => {}
                WAIT_OBJECT_0 => interrupted = Some(io::Error::new(io::ErrorKind::ConnectionAborted, "connection closed")),
                WAIT_TIMEOUT => interrupted = Some(io::Error::new(io::ErrorKind::TimedOut, "pipe deadline expired")),
                WAIT_FAILED => interrupted = Some(io::Error::last_os_error()),
                _ => interrupted = Some(io::Error::other("unexpected pipe wait result")),
            }
            if interrupted.is_some() {
                unsafe {
                    CancelIoEx(handle, &overlapped);
                }
            }
        }
        // Always drain completion before returning: the kernel must no longer
        // hold pointers into the caller's buffer or this OVERLAPPED record.
        let completed = unsafe { GetOverlappedResult(handle, &overlapped, &mut bytes, 1) };
        if completed != 0 {
            if interrupted.as_ref().is_some_and(|e| e.kind() != io::ErrorKind::TimedOut) {
                return Err(interrupted.unwrap());
            }
            return Ok(bytes as usize); // completion won a timeout race: keep data
        }
        let error = unsafe { GetLastError() };
        if error == ERROR_OPERATION_ABORTED {
            return Err(interrupted.unwrap_or_else(|| io::Error::new(io::ErrorKind::ConnectionAborted, "pipe operation cancelled")));
        }
        if !sending && [ERROR_BROKEN_PIPE, ERROR_HANDLE_EOF].contains(&error) {
            return Ok(0);
        }
        Err(io::Error::from_raw_os_error(error as i32))
    }
}
