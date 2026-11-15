//! Rede do sandbox: namespace próprio com `lo` e, no modo aberta, o TUN
//! `sandmux0`, cujo descritor vai para o supervisor, onde roda o gateway.
//!
//! ```text
//! sandbox 10.0.2.100 --sandmux0 (TUN)--> supervisor: gateway --> host
//!   rota padrão 10.0.2.2, DNS 10.0.2.3, máquina 10.0.2.254
//! ```

use std::io;
use std::net::Ipv4Addr;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

/// Nome da interface do sandbox.
pub const IFACE: &str = "sandmux0";

fn check(ret: libc::c_int) -> io::Result<libc::c_int> {
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ret)
    }
}

fn ifreq(name: &str) -> libc::ifreq {
    // SAFETY: ifreq é uma struct C sem invariantes; zero é válido.
    let mut r: libc::ifreq = unsafe { std::mem::zeroed() };
    for (d, s) in r.ifr_name.iter_mut().zip(name.bytes()) {
        *d = s as libc::c_char;
    }
    r
}

fn sockaddr(ip: Ipv4Addr) -> libc::sockaddr {
    let sin = libc::sockaddr_in {
        sin_family: libc::AF_INET as libc::sa_family_t,
        sin_port: 0,
        sin_addr: libc::in_addr {
            s_addr: u32::from(ip).to_be(),
        },
        sin_zero: [0; 8],
    };
    // SAFETY: sockaddr_in e sockaddr têm o mesmo tamanho no Linux.
    unsafe { std::mem::transmute::<libc::sockaddr_in, libc::sockaddr>(sin) }
}

/// `struct rtentry` do kernel (`linux/route.h`).
#[repr(C)]
struct RtEntry {
    rt_pad1: libc::c_ulong,
    rt_dst: libc::sockaddr,
    rt_gateway: libc::sockaddr,
    rt_genmask: libc::sockaddr,
    rt_flags: libc::c_ushort,
    rt_pad2: libc::c_short,
    rt_pad3: libc::c_ulong,
    rt_pad4: *mut libc::c_void,
    rt_metric: libc::c_short,
    rt_dev: *mut libc::c_char,
    rt_mtu: libc::c_ulong,
    rt_window: libc::c_ulong,
    rt_irtt: libc::c_ushort,
}

struct Ctl(OwnedFd);

impl Ctl {
    fn new() -> io::Result<Ctl> {
        // SAFETY: socket sem ponteiros.
        let fd = check(unsafe {
            libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0)
        })?;
        // SAFETY: descritor recém-criado, nosso.
        Ok(Ctl(unsafe { OwnedFd::from_raw_fd(fd) }))
    }

    fn ioctl<T>(&self, req: libc::c_ulong, arg: &mut T) -> io::Result<()> {
        // SAFETY: arg aponta para a struct que o pedido espera.
        check(unsafe { libc::ioctl(self.0.as_raw_fd(), req as _, arg as *mut T) }).map(|_| ())
    }

    fn up(&self, name: &str) -> io::Result<()> {
        let mut r = ifreq(name);
        self.ioctl(libc::SIOCGIFFLAGS, &mut r)?;
        // SAFETY: o campo de flags foi preenchido pelo kernel.
        unsafe {
            r.ifr_ifru.ifru_flags |= (libc::IFF_UP | libc::IFF_RUNNING) as libc::c_short;
        }
        self.ioctl(libc::SIOCSIFFLAGS, &mut r)
    }
}

/// Configura a rede no namespace novo. Só sobe o `lo` no modo sem rede; no
/// modo aberta cria `sandmux0` e devolve o descritor do TUN.
pub fn setup(open: bool) -> io::Result<Option<OwnedFd>> {
    let ctl = Ctl::new()?;
    ctl.up("lo").map_err(|e| {
        io::Error::new(
            e.kind(),
            sandmux_spec::trf!("subir o lo: {e}", "bring up lo: {e}"),
        )
    })?;
    if !open {
        return Ok(None);
    }
    let tun = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/net/tun")
        .map_err(|e| {
            io::Error::new(
                e.kind(),
                sandmux_spec::trf!("abrir /dev/net/tun: {e}", "open /dev/net/tun: {e}"),
            )
        })?;
    let tun = OwnedFd::from(tun);
    let mut r = ifreq(IFACE);
    r.ifr_ifru.ifru_flags = (libc::IFF_TUN | libc::IFF_NO_PI) as libc::c_short;
    // SAFETY: TUNSETIFF recebe um ifreq.
    check(unsafe { libc::ioctl(tun.as_raw_fd(), libc::TUNSETIFF as _, &mut r) }).map_err(|e| {
        io::Error::new(
            e.kind(),
            sandmux_spec::trf!("criar {IFACE}: {e}", "create {IFACE}: {e}"),
        )
    })?;

    let ctx =
        |step: &'static str| move |e: io::Error| io::Error::new(e.kind(), format!("{step}: {e}"));
    let mut a = ifreq(IFACE);
    a.ifr_ifru.ifru_addr = sockaddr(Ipv4Addr::new(10, 0, 2, 100));
    ctl.ioctl(libc::SIOCSIFADDR, &mut a)
        .map_err(ctx(sandmux_spec::tr!("endereço", "address")))?;
    let mut m = ifreq(IFACE);
    m.ifr_ifru.ifru_netmask = sockaddr(Ipv4Addr::new(255, 255, 255, 0));
    ctl.ioctl(libc::SIOCSIFNETMASK, &mut m)
        .map_err(ctx(sandmux_spec::tr!("máscara", "netmask")))?;
    let mut mtu = ifreq(IFACE);
    mtu.ifr_ifru.ifru_mtu = 1500;
    ctl.ioctl(libc::SIOCSIFMTU, &mut mtu).map_err(ctx("MTU"))?;
    ctl.up(IFACE).map_err(ctx(sandmux_spec::tr!(
        "subir a interface",
        "bring up the interface"
    )))?;

    let mut rt = RtEntry {
        rt_pad1: 0,
        rt_dst: sockaddr(Ipv4Addr::UNSPECIFIED),
        rt_gateway: sockaddr(Ipv4Addr::new(10, 0, 2, 2)),
        rt_genmask: sockaddr(Ipv4Addr::UNSPECIFIED),
        rt_flags: libc::RTF_UP | libc::RTF_GATEWAY,
        rt_pad2: 0,
        rt_pad3: 0,
        rt_pad4: std::ptr::null_mut(),
        rt_metric: 0,
        rt_dev: std::ptr::null_mut(),
        rt_mtu: 0,
        rt_window: 0,
        rt_irtt: 0,
    };
    ctl.ioctl(libc::SIOCADDRT, &mut rt)
        .map_err(ctx(sandmux_spec::tr!("rota padrão", "default route")))?;
    Ok(Some(tun))
}

/// Par de sockets para levar o TUN do filho ao supervisor.
pub fn pair() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0 as RawFd; 2];
    // SAFETY: fds aponta para dois inteiros.
    check(unsafe {
        libc::socketpair(
            libc::AF_UNIX,
            libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
            0,
            fds.as_mut_ptr(),
        )
    })?;
    // SAFETY: descritores recém-criados, nossos.
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

const CMSG_BUF: usize = 64;

/// Manda um descritor por SCM_RIGHTS.
pub fn send_fd(sock: &OwnedFd, fd: &OwnedFd) -> io::Result<()> {
    send_fd_tagged(sock, fd, b'T')
}

/// Manda um descritor por SCM_RIGHTS, com um byte de dados.
pub fn send_fd_tagged(sock: &OwnedFd, fd: &impl AsRawFd, tag: u8) -> io::Result<()> {
    let mut byte = [tag];
    let mut iov = libc::iovec {
        iov_base: byte.as_mut_ptr().cast(),
        iov_len: 1,
    };
    let mut buf = [0u8; CMSG_BUF];
    // SAFETY: msghdr zerado é válido; os ponteiros apontam para buffers vivos
    // até o fim da chamada, e o cmsg cabe no buffer.
    unsafe {
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = buf.as_mut_ptr().cast();
        msg.msg_controllen = libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) as _;
        let c = libc::CMSG_FIRSTHDR(&msg);
        (*c).cmsg_level = libc::SOL_SOCKET;
        (*c).cmsg_type = libc::SCM_RIGHTS;
        (*c).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as _;
        std::ptr::write_unaligned(libc::CMSG_DATA(c).cast::<RawFd>(), fd.as_raw_fd());
        check(libc::sendmsg(sock.as_raw_fd(), &msg, 0) as libc::c_int)?;
    }
    Ok(())
}

/// Recebe um descritor por SCM_RIGHTS. `None` quando o outro lado fechou sem
/// mandar nada.
pub fn recv_fd(sock: &OwnedFd) -> io::Result<Option<OwnedFd>> {
    let mut byte = [0u8];
    let mut iov = libc::iovec {
        iov_base: byte.as_mut_ptr().cast(),
        iov_len: 1,
    };
    let mut buf = [0u8; CMSG_BUF];
    // SAFETY: como em send_fd; o kernel preenche o cmsg dentro do buffer.
    unsafe {
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = buf.as_mut_ptr().cast();
        msg.msg_controllen = CMSG_BUF as _;
        let n = loop {
            let n = libc::recvmsg(sock.as_raw_fd(), &mut msg, libc::MSG_CMSG_CLOEXEC);
            if n < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            break n;
        };
        check(n as libc::c_int)?;
        if n == 0 {
            return Ok(None);
        }
        let c = libc::CMSG_FIRSTHDR(&msg);
        if c.is_null() || (*c).cmsg_level != libc::SOL_SOCKET || (*c).cmsg_type != libc::SCM_RIGHTS
        {
            return Ok(None);
        }
        let fd = std::ptr::read_unaligned(libc::CMSG_DATA(c).cast::<RawFd>());
        Ok(Some(OwnedFd::from_raw_fd(fd)))
    }
}
