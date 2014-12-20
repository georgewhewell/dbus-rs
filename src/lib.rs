extern crate libc;

pub use ffi::DBusBusType as BusType;
pub use ffi::DBusNameFlag as NameFlag;
pub use ffi::DBusRequestNameReply as RequestNameReply;
pub use ffi::DBusReleaseNameReply as ReleaseNameReply;
pub use ffi::DBusMessageType as MessageType;

use std::c_str::CString;
use std::ptr;
use std::collections::DList;

mod ffi;

pub mod prop;

static INITDBUS: std::sync::Once = std::sync::ONCE_INIT;

fn init_dbus() {
    INITDBUS.doit(|| {
        if unsafe { ffi::dbus_threads_init_default() } == 0 {
            panic!("Out of memory when trying to initialize DBus library!");
        }
    });
}


pub struct Error {
    e: ffi::DBusError,
}

fn c_str_to_slice(c: & *const libc::c_char) -> Option<&str> {
    if *c == ptr::null() { None }
    else { std::str::from_utf8( unsafe { std::mem::transmute::<_,&[u8]>(
        std::raw::Slice { data: *c as *const u8, len: libc::strlen(*c) as uint }
    )})}
}

impl Error {

    pub fn new(e: ffi::DBusError) -> Error {
        Error { e: e }
    }

    pub fn new_custom(name: &str, message: &str) -> Error {
        let n = name.to_c_str();
        let m = message.replace("%","%%").to_c_str();
        let mut e = Error::empty();

        unsafe { ffi::dbus_set_error(e.get_mut(), n.as_ptr(), m.as_ptr()) };
        e
    }

    fn empty() -> Error {
        init_dbus();
        let mut e = ffi::DBusError {
            name: ptr::null(),
            message: ptr::null(),
            dummy: 0,
            padding1: ptr::null()
        };
        unsafe { ffi::dbus_error_init(&mut e); }
        Error{ e: e }
    }

    pub fn get(&self) -> &ffi::DBusError { &self.e }

    pub fn name(&self) -> Option<&str> {
        c_str_to_slice(&self.e.name)
    }

    pub fn message(&self) -> Option<&str> {
        c_str_to_slice(&self.e.message)
    }

    fn get_mut(&mut self) -> &mut ffi::DBusError { &mut self.e }
}

impl Drop for Error {
    fn drop(&mut self) {
        unsafe { ffi::dbus_error_free(&mut self.e); }
    }
}

impl std::fmt::Show for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "DBus error: {} ({})", self.message().unwrap_or(""),
            self.name().unwrap_or(""))
    }
}

impl std::error::Error for Error {
    fn description(&self) -> &str { "DBus error" }
    fn detail(&self) -> Option<String> { self.message().map(|x| x.to_string()) }
}

fn new_dbus_message_iter() -> ffi::DBusMessageIter {
    ffi::DBusMessageIter {
        dummy1: ptr::null_mut(),
        dummy2: ptr::null_mut(),
        dummy3: 0,
        dummy4: 0,
        dummy5: 0,
        dummy6: 0,
        dummy7: 0,
        dummy8: 0,
        dummy9: 0,
        dummy10: 0,
        dummy11: 0,
        pad1: 0,
        pad2: 0,
        pad3: ptr::null_mut(),
    }
}

#[deriving(Show, PartialEq, PartialOrd, Clone)]
pub enum MessageItem {
    Array(Vec<MessageItem>, int),
    Variant(Box<MessageItem>),
    DictEntry(Box<MessageItem>, Box<MessageItem>),
    Str(String),
    Bool(bool),
    Byte(u8),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
}

fn iter_get_basic(i: &mut ffi::DBusMessageIter) -> i64 {
    let mut c: i64 = 0;
    unsafe {
        let p: *mut libc::c_void = std::mem::transmute(&mut c);
        ffi::dbus_message_iter_get_basic(i, p);
    }
    c
}

fn iter_append_array(i: &mut ffi::DBusMessageIter, a: &[MessageItem], t: int) {
    let mut subiter = new_dbus_message_iter();

    // TODO: This works for simple dictionaries. Not so well for dictionaries of dictionaries, probably.
    let atype =
        if t <= 0 {
            match &a[0] {
                &MessageItem::DictEntry(ref k, ref v) => format!("{{{}{}}}",
                    k.array_type() as u8 as char, v.array_type() as u8 as char),
                _ => format!("{}", a[0].array_type() as u8 as char),
            }
        }
        else { format!("{}", t as u8 as char) }
        .to_c_str();

    assert!(unsafe { ffi::dbus_message_iter_open_container(i, ffi::DBUS_TYPE_ARRAY, atype.as_ptr(), &mut subiter) } != 0);
    for item in a.iter() {
        assert!(t < 0 || item.array_type() == t as int);
        item.iter_append(&mut subiter);
    }
    assert!(unsafe { ffi::dbus_message_iter_close_container(i, &mut subiter) } != 0);
}

fn iter_append_variant(i: &mut ffi::DBusMessageIter, a: &MessageItem) {
    let mut subiter = new_dbus_message_iter();
    let atype = format!("{}", a.array_type() as u8 as char).to_c_str();
    assert!(unsafe { ffi::dbus_message_iter_open_container(i, ffi::DBUS_TYPE_VARIANT, atype.as_ptr(), &mut subiter) } != 0);
    a.iter_append(&mut subiter);
    assert!(unsafe { ffi::dbus_message_iter_close_container(i, &mut subiter) } != 0);
}

fn iter_append_dict(i: &mut ffi::DBusMessageIter, k: &MessageItem, v: &MessageItem) {
    let mut subiter = new_dbus_message_iter();
    assert!(unsafe { ffi::dbus_message_iter_open_container(i, ffi::DBUS_TYPE_DICT_ENTRY, ptr::null(), &mut subiter) } != 0);
    k.iter_append(&mut subiter);
    v.iter_append(&mut subiter);
    assert!(unsafe { ffi::dbus_message_iter_close_container(i, &mut subiter) } != 0);
}

impl MessageItem {

    pub fn array_type(&self) -> int {
        let s = match self {
            &MessageItem::Str(_) => ffi::DBUS_TYPE_STRING,
            &MessageItem::Bool(_) => ffi::DBUS_TYPE_BOOLEAN,
            &MessageItem::Byte(_) => ffi::DBUS_TYPE_BYTE,
            &MessageItem::Int16(_) => ffi::DBUS_TYPE_INT16,
            &MessageItem::Int32(_) => ffi::DBUS_TYPE_INT32,
            &MessageItem::Int64(_) => ffi::DBUS_TYPE_INT64,
            &MessageItem::UInt16(_) => ffi::DBUS_TYPE_UINT16,
            &MessageItem::UInt32(_) => ffi::DBUS_TYPE_UINT32,
            &MessageItem::UInt64(_) => ffi::DBUS_TYPE_UINT64,
            &MessageItem::Array(_,_) => ffi::DBUS_TYPE_ARRAY,
            &MessageItem::Variant(_) => ffi::DBUS_TYPE_VARIANT,
            &MessageItem::DictEntry(_,_) => ffi::DBUS_TYPE_DICT_ENTRY,
        };
        s as int
    }

    fn from_iter(i: &mut ffi::DBusMessageIter) -> Vec<MessageItem> {
        let mut v = Vec::new();
        loop {
            let t = unsafe { ffi::dbus_message_iter_get_arg_type(i) };
            match t {
                ffi::DBUS_TYPE_INVALID => { return v },
                ffi::DBUS_TYPE_DICT_ENTRY => {
                    let mut subiter = new_dbus_message_iter();
                    unsafe { ffi::dbus_message_iter_recurse(i, &mut subiter) };
                    let a = MessageItem::from_iter(&mut subiter);
                    if a.len() != 2 { panic!("DBus dict entry error"); }
                    let mut a = a.into_iter();
                    let key = box a.next().unwrap();
                    let value = box a.next().unwrap();
                    v.push(MessageItem::DictEntry(key, value));
                }
                ffi::DBUS_TYPE_VARIANT => {
                    let mut subiter = new_dbus_message_iter();
                    unsafe { ffi::dbus_message_iter_recurse(i, &mut subiter) };
                    let a = MessageItem::from_iter(&mut subiter);
                    if a.len() != 1 { panic!("DBus variant error"); }
                    v.push(MessageItem::Variant(box a.into_iter().next().unwrap()));
                }
                ffi::DBUS_TYPE_ARRAY => {
                    let mut subiter = new_dbus_message_iter();
                    unsafe { ffi::dbus_message_iter_recurse(i, &mut subiter) };
                    let a = MessageItem::from_iter(&mut subiter);
                    let t = if a.len() > 0 { a[0].array_type() } else { 0 };
                    v.push(MessageItem::Array(a, t));
                },
                ffi::DBUS_TYPE_STRING => {
                    let mut c: *const libc::c_char = ptr::null();
                    let s = unsafe {
                        let p: *mut libc::c_void = std::mem::transmute(&mut c);
                        ffi::dbus_message_iter_get_basic(i, p);
                        CString::new(c, false)
                    };
                    v.push(MessageItem::Str(s.to_string()));
                },
                ffi::DBUS_TYPE_BOOLEAN => v.push(MessageItem::Bool((iter_get_basic(i) as u32) != 0)),
                ffi::DBUS_TYPE_BYTE => v.push(MessageItem::Byte(iter_get_basic(i) as u8)),
                ffi::DBUS_TYPE_INT16 => v.push(MessageItem::Int16(iter_get_basic(i) as i16)),
                ffi::DBUS_TYPE_INT32 => v.push(MessageItem::Int32(iter_get_basic(i) as i32)),
                ffi::DBUS_TYPE_INT64 => v.push(MessageItem::Int64(iter_get_basic(i) as i64)),
                ffi::DBUS_TYPE_UINT16 => v.push(MessageItem::UInt16(iter_get_basic(i) as u16)),
                ffi::DBUS_TYPE_UINT32 => v.push(MessageItem::UInt32(iter_get_basic(i) as u32)),
                ffi::DBUS_TYPE_UINT64 => v.push(MessageItem::UInt64(iter_get_basic(i) as u64)),

                _ => { panic!("DBus unsupported message type {} ({})", t, t as u8 as char); }
            }
            unsafe { ffi::dbus_message_iter_next(i) };
        }
    }

    fn iter_append_basic(&self, i: &mut ffi::DBusMessageIter, v: i64) {
        let t = self.array_type();
        unsafe {
            let p: *const libc::c_void = std::mem::transmute(&v);
            ffi::dbus_message_iter_append_basic(i, t as libc::c_int, p);
        }
    }

    fn iter_append(&self, i: &mut ffi::DBusMessageIter) {
        match self {
            &MessageItem::Str(ref s) => unsafe {
                let c = s.to_c_str();
                let p = std::mem::transmute(&c);
                ffi::dbus_message_iter_append_basic(i, ffi::DBUS_TYPE_STRING, p);
            },
            &MessageItem::Bool(b) => self.iter_append_basic(i, b as i64),
            &MessageItem::Byte(b) => self.iter_append_basic(i, b as i64),
            &MessageItem::Int16(b) => self.iter_append_basic(i, b as i64),
            &MessageItem::Int32(b) => self.iter_append_basic(i, b as i64),
            &MessageItem::Int64(b) => self.iter_append_basic(i, b as i64),
            &MessageItem::UInt16(b) => self.iter_append_basic(i, b as i64),
            &MessageItem::UInt32(b) => self.iter_append_basic(i, b as i64),
            &MessageItem::UInt64(b) => self.iter_append_basic(i, b as i64),
            &MessageItem::Array(ref b, t) => iter_append_array(i, b.as_slice(), t),
            &MessageItem::Variant(ref b) => iter_append_variant(i, &**b),
            &MessageItem::DictEntry(ref k, ref v) => iter_append_dict(i, &**k, &**v),
        }
    }

    fn copy_to_iter(i: &mut ffi::DBusMessageIter, v: &[MessageItem]) {
        for item in v.iter() {
            item.iter_append(i);
        }
    }
}

pub struct Message {
    msg: *mut ffi::DBusMessage,
}

impl Message {
    pub fn new_method_call(destination: &str, path: &str, iface: &str, method: &str) -> Option<Message> {
        init_dbus();
        let (d, p, i, m) = (destination.to_c_str(), path.to_c_str(), iface.to_c_str(), method.to_c_str());
        let ptr = unsafe {
            ffi::dbus_message_new_method_call(d.as_ptr(), p.as_ptr(), i.as_ptr(), m.as_ptr())
        };
        if ptr == ptr::null_mut() { None } else { Some(Message { msg: ptr} ) }
    }

    pub fn new_signal(path: &str, iface: &str, method: &str) -> Option<Message> {
        init_dbus();
        let (p, i, m) = (path.to_c_str(), iface.to_c_str(), method.to_c_str());
        let ptr = unsafe {
            ffi::dbus_message_new_signal(p.as_ptr(), i.as_ptr(), m.as_ptr())
        };
        if ptr == ptr::null_mut() { None } else { Some(Message { msg: ptr} ) }
    }

    pub fn new_method_return(m: &Message) -> Option<Message> {
        let ptr = unsafe { ffi::dbus_message_new_method_return(m.msg) };
        if ptr == ptr::null_mut() { None } else { Some(Message { msg: ptr} ) }
    }

    pub fn new_error(m: &Message, error_name: &str, error_message: &str) -> Option<Message> {
        let (en, em) = (error_name.to_c_str(), error_message.to_c_str());
        let ptr = unsafe { ffi::dbus_message_new_error(m.msg, en.as_ptr(), em.as_ptr()) };
        if ptr == ptr::null_mut() { None } else { Some(Message { msg: ptr} ) }
    }

    fn from_ptr(ptr: *mut ffi::DBusMessage, add_ref: bool) -> Message {
        if add_ref {
            unsafe { ffi::dbus_message_ref(ptr) };
        }
        Message { msg: ptr }
    }

    pub fn get_items(&mut self) -> Vec<MessageItem> {
        let mut i = new_dbus_message_iter();
        match unsafe { ffi::dbus_message_iter_init(self.msg, &mut i) } {
            0 => Vec::new(),
            _ => MessageItem::from_iter(&mut i)
        }
    }

    pub fn append_items(&mut self, v: &[MessageItem]) {
        let mut i = new_dbus_message_iter();
        unsafe { ffi::dbus_message_iter_init_append(self.msg, &mut i) };
        MessageItem::copy_to_iter(&mut i, v);
    }

    pub fn msg_type(&self) -> MessageType {
        unsafe { std::mem::transmute(ffi::dbus_message_get_type(self.msg)) }
    }

    pub fn sender(&self) -> Option<String> {
        let s = unsafe { ffi::dbus_message_get_sender(self.msg) };
        c_str_to_slice(&s).map(|s| s.to_string())
    }

    pub fn headers(&self) -> (MessageType, Option<String>, Option<String>, Option<String>) {
        let p = unsafe { ffi::dbus_message_get_path(self.msg) };
        let i = unsafe { ffi::dbus_message_get_interface(self.msg) };
        let m = unsafe { ffi::dbus_message_get_member(self.msg) };
        (self.msg_type(),
         c_str_to_slice(&p).map(|s| s.to_string()),
         c_str_to_slice(&i).map(|s| s.to_string()),
         c_str_to_slice(&m).map(|s| s.to_string()))
    }

    pub fn as_result(&mut self) -> Result<&mut Message, Error> {
        let mut e = Error::empty();
        if unsafe { ffi::dbus_set_error_from_message(e.get_mut(), self.msg) } != 0 { Err(e) }
        else { Ok(self) }
    }
}

impl Drop for Message {
    fn drop(&mut self) {
        unsafe {
            ffi::dbus_message_unref(self.msg);
        }
    }
}

impl std::fmt::Show for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.headers())
    }
}

#[deriving(Show)]
pub enum ConnectionItem {
    Nothing,
    MethodCall(Message),
    Signal(Message),
}

pub struct ConnectionItems<'a> {
    c: &'a mut Connection,
    timeout_ms: int,
}

impl<'a> Iterator<ConnectionItem> for ConnectionItems<'a> {
    fn next(&mut self) -> Option<ConnectionItem> {
        loop {
            let i = self.c.i.pending_items.pop_front();
            if i.is_some() { return i; }

            let r = unsafe { ffi::dbus_connection_read_write_dispatch(self.c.i.conn, self.timeout_ms as libc::c_int) };
            if !self.c.i.pending_items.is_empty() { continue };

            if r == 0 { return None; }
            return Some(ConnectionItem::Nothing);
        }
    }
}

/* Since we register callbacks with userdata pointers,
   we need to make sure the connection pointer does not move around.
   Hence this extra indirection. */
struct IConnection {
    conn: *mut ffi::DBusConnection,
    pending_items: DList<ConnectionItem>,
}

pub struct Connection {
    i: Box<IConnection>,
}


extern "C" fn filter_message_cb(conn: *mut ffi::DBusConnection, msg: *mut ffi::DBusMessage,
    user_data: *mut libc::c_void) -> ffi::DBusHandlerResult {

    let m = Message::from_ptr(msg, true);
    let mut c = Connection { i: unsafe { std::mem::transmute(user_data) } };
    assert_eq!(c.i.conn, conn);

    let mtype: ffi::DBusMessageType = unsafe { std::mem::transmute(ffi::dbus_message_get_type(msg)) };
    let r = match mtype {
        ffi::DBusMessageType::Signal => {
            c.i.pending_items.push_back(ConnectionItem::Signal(m));
            ffi::DBusHandlerResult::Handled
        }
        _ => ffi::DBusHandlerResult::NotYetHandled,
    };

    unsafe { std::mem::forget(c) };
    r
}
/*
extern "C" fn object_path_message_cb(_: *mut ffi::DBusConnection, _: *mut ffi::DBusMessage,
    _: *mut libc::c_void) -> ffi::DBusHandlerResult {

    ffi::DBusMessageType::MethodCall => c.i.pending_items.push_back(ConnectionItem::MethodCall(m)),

    /* Everything is handled by the filter, so this is just a dummy function now. */
    ffi::DBusHandlerResult::NotYetHandled
}
*/

extern "C" fn object_path_message_cb(conn: *mut ffi::DBusConnection, msg: *mut ffi::DBusMessage,
    user_data: *mut libc::c_void) -> ffi::DBusHandlerResult {

    let m = Message::from_ptr(msg, true);
    let mut c = Connection { i: unsafe { std::mem::transmute(user_data) } };
    assert!(c.i.conn == conn);
    c.i.pending_items.push_back(ConnectionItem::MethodCall(m));
    unsafe { std::mem::forget(c) };
    ffi::DBusHandlerResult::Handled
}

impl Connection {
    pub fn get_private(bus: BusType) -> Result<Connection, Error> {
        let mut e = Error::empty();
        let conn = unsafe { ffi::dbus_bus_get_private(bus, e.get_mut()) };
        if conn == ptr::null_mut() {
            return Err(e)
        }
        let c = Connection { i: box IConnection { conn: conn, pending_items: DList::new() } };

        /* No, we don't want our app to suddenly quit if dbus goes down */
        unsafe { ffi::dbus_connection_set_exit_on_disconnect(conn, 0) };
        assert!(unsafe {
            ffi::dbus_connection_add_filter(c.i.conn, Some(filter_message_cb), std::mem::transmute(&*c.i), None)
        } != 0);
        Ok(c)
    }

    pub fn send_with_reply_and_block(&mut self, message: Message, timeout_ms: int) -> Result<Message, Error> {
        let mut e = Error::empty();
        let response = unsafe {
            ffi::dbus_connection_send_with_reply_and_block(self.i.conn, message.msg, timeout_ms as libc::c_int, e.get_mut())
        };
        if response == ptr::null_mut() {
            return Err(e);
        }
        Ok(Message::from_ptr(response, false))
    }

    pub fn send(&mut self, message: Message) -> Result<(),()> {
        let r = unsafe { ffi::dbus_connection_send(self.i.conn, message.msg, ptr::null_mut()) };
        if r == 0 { return Err(()); }
        unsafe { ffi::dbus_connection_flush(self.i.conn) };
        Ok(())
    }

    pub fn unique_name(&self) -> String {
        let c = unsafe { ffi::dbus_bus_get_unique_name(self.i.conn) };
        if c == ptr::null() {
            return "".to_string();
        }
        unsafe { CString::new(c, false) }.as_str().unwrap_or("").to_string()
    }

    pub fn iter(&mut self, timeout_ms: int) -> ConnectionItems {
        ConnectionItems {
            c: self,
            timeout_ms: timeout_ms,
        }
    }

    pub fn register_object_path(&mut self, path: &str) -> Result<(), Error> {
        let mut e = Error::empty();
        let p = path.to_c_str();
        let vtable = ffi::DBusObjectPathVTable {
            unregister_function: None,
            message_function: Some(object_path_message_cb),
            dbus_internal_pad1: None,
            dbus_internal_pad2: None,
            dbus_internal_pad3: None,
            dbus_internal_pad4: None,
        };
        let r = unsafe {
            let user_data: *mut libc::c_void = std::mem::transmute(&*self.i);
            ffi::dbus_connection_try_register_object_path(self.i.conn, p.as_ptr(), &vtable, user_data, e.get_mut())
        };
        if r == 0 { Err(e) } else { Ok(()) }
    }

    pub fn unregister_object_path(&mut self, path: &str) {
        let p = path.to_c_str();
        let r = unsafe { ffi::dbus_connection_unregister_object_path(self.i.conn, p.as_ptr()) };
        if r == 0 { panic!("Out of memory"); }
    }

    pub fn register_name(&mut self, name: &str, flags: u32) -> Result<RequestNameReply, Error> {
        let mut e = Error::empty();
        let n = name.to_c_str();
        let r = unsafe { ffi::dbus_bus_request_name(self.i.conn, n.as_ptr(), flags, e.get_mut()) };
        if r == -1 { Err(e) } else { Ok(unsafe { std::mem::transmute(r) }) }
    }

    pub fn release_name(&mut self, name: &str) -> Result<ReleaseNameReply, Error> {
        let mut e = Error::empty();
        let n = name.to_c_str();
        let r = unsafe { ffi::dbus_bus_release_name(self.i.conn, n.as_ptr(), e.get_mut()) };
        if r == -1 { Err(e) } else { Ok(unsafe { std::mem::transmute(r) }) }
    }

    pub fn add_match(&mut self, rule: &str) -> Result<(), Error> {
        let mut e = Error::empty();
        let n = rule.to_c_str();
        unsafe { ffi::dbus_bus_add_match(self.i.conn, n.as_ptr(), e.get_mut()) };
        if e.name().is_some() { Err(e) } else { Ok(()) }
    }

    pub fn remove_match(&mut self, rule: &str) -> Result<(), Error> {
        let mut e = Error::empty();
        let n = rule.to_c_str();
        unsafe { ffi::dbus_bus_remove_match(self.i.conn, n.as_ptr(), e.get_mut()) };
        if e.name().is_some() { Err(e) } else { Ok(()) }
    }

}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            ffi::dbus_connection_close(self.i.conn);
            ffi::dbus_connection_unref(self.i.conn);
        }
    }
}

#[cfg(test)]
mod test {
    use super::{Connection, Message, BusType, MessageItem, ConnectionItem, NameFlag,
        RequestNameReply, ReleaseNameReply};

    #[test]
    fn connection() {
        let c = Connection::get_private(BusType::Session).unwrap();
        let n = c.unique_name();
        assert!(n.as_slice().starts_with(":1."));
        println!("Connected to DBus, unique name: {}", n);
    }

    #[test]
    fn invalid_message() {
        let mut c = Connection::get_private(BusType::Session).unwrap();
        let m = Message::new_method_call("foo.bar", "/", "foo.bar", "FooBar").unwrap();
        let e = c.send_with_reply_and_block(m, 2000).err().unwrap();
        assert!(e.name().unwrap() == "org.freedesktop.DBus.Error.ServiceUnknown");
    }

    #[test]
    fn message_listnames() {
        let mut c = Connection::get_private(BusType::Session).unwrap();
        let m = Message::new_method_call("org.freedesktop.DBus", "/", "org.freedesktop.DBus", "ListNames").unwrap();
        let mut r = c.send_with_reply_and_block(m, 2000).unwrap();
        let reply = r.get_items();
        println!("{}", reply);
    }

    #[test]
    fn message_namehasowner() {
        let mut c = Connection::get_private(BusType::Session).unwrap();
        let mut m = Message::new_method_call("org.freedesktop.DBus", "/", "org.freedesktop.DBus", "NameHasOwner").unwrap();
        m.append_items(&[MessageItem::Str("org.freedesktop.DBus".to_string())]);
        let mut r = c.send_with_reply_and_block(m, 2000).unwrap();
        let reply = r.get_items();
        println!("{}", reply);
        assert_eq!(reply, vec!(MessageItem::Bool(true)));
    }

    #[test]
    fn object_path() {
        let (tx, rx) = channel();
        spawn(move || {
            let mut c = Connection::get_private(BusType::Session).unwrap();
            c.register_object_path("/hello").unwrap();
            // println!("Waiting...");
            tx.send(c.unique_name());
            loop {
                let n = c.iter(1000).next();
                if n.is_none() { break; }
                let n = n.unwrap();

                // println!("Found message... ({})", n);
                match n {
                    ConnectionItem::MethodCall(ref m) => {
                        let reply = Message::new_method_return(m).unwrap();
                        c.send(reply).unwrap();
                        break;
                    }
                    _ => {}
                }
            }
            c.unregister_object_path("/hello");
        });

        let mut c = Connection::get_private(BusType::Session).unwrap();
        let n = rx.recv();
        let m = Message::new_method_call(n.as_slice(), "/hello", "com.example.hello", "Hello").unwrap();
        println!("Sending...");
        let mut r = c.send_with_reply_and_block(m, 8000).unwrap();
        let reply = r.get_items();
        println!("{}", reply);
    }

    #[test]
    fn message_types() {
        let mut c = Connection::get_private(BusType::Session).unwrap();
        c.register_object_path("/hello").unwrap();
        let mut m = Message::new_method_call(c.unique_name().as_slice(), "/hello", "com.example.hello", "Hello").unwrap();
        m.append_items(&[
            MessageItem::UInt16(2000),
            MessageItem::Array(vec!(MessageItem::Byte(129)), -1),
            MessageItem::UInt64(987654321),
            MessageItem::Int32(-1),
            MessageItem::Str("Hello world".to_string()),
            MessageItem::Array(vec!(
                MessageItem::DictEntry(box MessageItem::UInt32(123543), box MessageItem::Bool(true))
            ), -1)
        ]);
        let sending = format!("{}", m.get_items());
        println!("Sending {}", sending);
        c.send(m).unwrap();

        for n in c.iter(1000) {
            match n {
                ConnectionItem::MethodCall(mut m) => {
                    let receiving = format!("{}", m.get_items());
                    println!("Receiving {}", receiving);
                    assert_eq!(sending, receiving);
                    break;
                }
                _ => println!("Got {}", n),
            }
        }
    }

    #[test]
    fn register_name() {
        use std::rand;
        let mut c = Connection::get_private(BusType::Session).unwrap();
        let n = format!("com.example.hello.test{}", rand::random::<u32>());
        assert_eq!(c.register_name(n.as_slice(), NameFlag::ReplaceExisting as u32).unwrap(), RequestNameReply::PrimaryOwner);
        assert_eq!(c.release_name(n.as_slice()).unwrap(), ReleaseNameReply::Released);
    }

    #[test]
    fn signal() {
        let mut c = Connection::get_private(BusType::Session).unwrap();
        let iface = "com.example.signaltest";
        let mstr = format!("interface='{}',member='ThisIsASignal'", iface);
        c.add_match(mstr.as_slice()).unwrap();
        let m = Message::new_signal("/mysignal", iface, "ThisIsASignal").unwrap();
        let uname = c.unique_name();
        c.send(m).unwrap();
        for n in c.iter(1000) {
            match n {
                ConnectionItem::Signal(s) => {
                    let (_, p, i, m) = s.headers();
                    match (p.unwrap().as_slice(), i.unwrap().as_slice(), m.unwrap().as_slice()) {
                        ("/mysignal", "com.example.signaltest", "ThisIsASignal") => {
                            assert_eq!(s.sender().unwrap(), uname);
                            break;
                        },
                        (_, _, _) => println!("Other signal: {}", s.headers()),
                    }
                }
                _ => {},
            }
        }
        c.remove_match(mstr.as_slice()).unwrap();
    }

}

