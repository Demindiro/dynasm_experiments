//! # JIT compiler & interpreter for ´
//!
//! This program is a quick experiment to (naïvely) test the performance difference
//! between interpreters and JIT compilers for simple programs.
//!
//! ## ´
//!
//! ´ is a derivative of [\`]. It is modified such that finite loops are possible.
//!
//! Instructions still use backticks since forward ticks are not ASCII and hence annoying.
//!
//! ## Instructions
//!
//! A and B are integer constants, \[X\] denotes an address dereference,  V is the last assigned
//! value, P is the instruction pointer.
//!
//! | Syntax |       Function      |
//! | ------ | ------------------- |
//! | A`+B   | [A] += B, V = [A]   |
//! | A`B    | [A] += [B], V = [A] |
//! | +A`+B  | [A] == V ? P += B   |
//! | +A`B   | [A] == V ? P += [B] |
//! | other  | ignored             |
//!
//! [\`]: https://esolangs.org/wiki/%60

use dynasmrt::{dynasm, DynasmApi, DynasmLabelApi};
use std::time::Instant;

pub enum Op {
	SetV(isize, isize),
	SetA(isize, isize),
	JmpV(isize, isize),
	JmpA(isize, isize),
}

fn parse_int(code: &mut Vec<u8>) -> Result<isize, ()> {
	let c = code.pop();
	let inv = if c == Some(b'-') {
		true
	} else {
		c.map(|c| code.push(c));
		false
	};
	let mut n = 0;
	while let Some(c) = code.pop() {
		if c < b'0' || b'9' < c {
			code.push(c);
			return Ok(if inv { -n } else { n });
		}
		n *= 10;
		n += (c - b'0') as isize;
	}
	Ok(if inv { -n } else { n })
}

fn parse(mut code: Vec<u8>) -> Vec<Op> {
	let mut ops = Vec::new();
	code.reverse();
	while let Some(b) = code.pop() {
		match b {
			b'+' => {
				if let Ok(a) = parse_int(&mut code) {
					if code.pop() != Some(b'`') {
						continue;
					}
					let chr = code.pop();
					let addr = chr != Some(b'+');
					if addr {
						chr.map(|c| code.push(c));
					}
					if let Ok(b) = parse_int(&mut code) {
						ops.push(if addr { Op::JmpA(a, b) } else { Op::JmpV(a, b) });
					}
				}
			}
			_ if b'-' == b || b'0' <= b && b <= b'9' => {
				code.push(b);
				if let Ok(a) = parse_int(&mut code) {
					if code.pop() != Some(b'`') {
						continue;
					}
					let chr = code.pop();
					let addr = chr != Some(b'+');
					if addr {
						chr.map(|c| code.push(c));
					}
					if let Ok(b) = parse_int(&mut code) {
						ops.push(if addr { Op::SetA(a, b) } else { Op::SetV(a, b) });
					}
				}
			}
			_ => (),
		}
	}
	ops
}

extern "C" fn print(v: isize) {
	use std::io::Write;
	let mut c = [0; 4];
	let c = char::from_u32(v as u32)
		.unwrap_or('\u{fffd}')
		.encode_utf8(&mut c);
	let _ = std::io::stdout().write(c.as_bytes());
}

fn run(ops: Vec<Op>) {
	let ops = &ops[..]; // This is faster. Don't ask me why.
	let mut i = 0;
	let mut tape = [0; 0x10000];
	let mut v = 0;
	let t = Instant::now();
	unsafe {
		while let Some(op) = ops.get(i) {
			i += 1;
			let (a, b) = match op {
				&Op::SetV(a, b) => (a, b),
				&Op::SetA(a, b) => (a, *tape.get_unchecked(b as usize)),
				&Op::JmpV(a, b) => {
					if a != v {
						i += b as usize - 1
					}
					continue;
				}
				&Op::JmpA(a, b) => {
					if a != v {
						i += *tape.get_unchecked(b as usize) as usize - 1
					}
					continue;
				}
			};
			*tape.get_unchecked_mut(a as usize) += b;
			v = *tape.get_unchecked(a as usize);
			(a == 0).then(|| print(v));
		}
	}
	eprintln!("{:?}", Instant::now() - t);
}

fn jit(ops: Vec<Op>) {
	let mut jit = dynasmrt::x64::Assembler::new().unwrap();
	let labels = core::iter::repeat_with(|| jit.new_dynamic_label())
		.take(ops.len())
		.collect::<Box<_>>();
	dynasm!(jit
		; push rbx
		; mov rbx, rdi
	);
	for (i, (op, &lbl)) in ops.into_iter().zip(labels.iter()).enumerate() {
		match op {
			Op::SetV(a, b) => {
				dynasm!(jit
					; =>lbl
					; mov rdi, QWORD b.try_into().unwrap()
					; add rdi, [rbx + (a * 8).try_into().unwrap()]
					; mov [rbx + (a * 8).try_into().unwrap()], rdi
				);
				(a == 0).then(|| dynasm!(jit ; mov rax, QWORD print as _ ; call rax));
			}
			Op::SetA(a, b) => {
				dynasm!(jit
					; =>lbl
					; mov rdi, [rbx + (b * 8).try_into().unwrap()]
					; add rdi, [rbx + (a * 8).try_into().unwrap()]
					; mov [rbx + (a * 8).try_into().unwrap()], rdi
				);
				(a == 0).then(|| dynasm!(jit ; mov rax, QWORD print as _ ; call rax));
			}
			Op::JmpV(a, b) => {
				dynasm!(jit
					; =>lbl
					; mov rax, [rbx + (a * 8).try_into().unwrap()]
					; cmp rdi, rax
					; jne =>labels[i - b as usize - 2]
				);
			}
			Op::JmpA(_a, _b) => {
				todo!()
			}
		}
	}
	dynasm!(jit
		; pop rbx
		; ret
	);
	let f = jit.finalize().unwrap();
	std::fs::write("/tmp/jit.out", &f[..]).unwrap();
	let f: extern "C" fn(*mut isize) =
		unsafe { core::mem::transmute(f.ptr(dynasmrt::AssemblyOffset(0))) };
	let t = Instant::now();
	f([0; 0x10000].as_mut_ptr());
	eprintln!("{:?}", Instant::now() - t);
}

/// JIT with heavier optimization
///
/// This JIT actually doesn't generate correct code in all cases but w/e.
fn jit2(ops: Vec<Op>) {
	let mut jit = dynasmrt::x64::Assembler::new().unwrap();
	let labels = core::iter::repeat_with(|| jit.new_dynamic_label())
		.take(ops.len())
		.collect::<Box<_>>();
	dynasm!(jit
		; push rbx
		; push rbp
		; push r15
		; push r14
		; push r13
		; push r12
		; mov rbx, rdi
	);

	use dynasmrt::{Register, x64::Rq, x64::X64Relocation, Assembler};
	let mut tape2reg = std::collections::HashMap::<isize, Rq>::new();
	let mut reg2tape = std::collections::HashMap::<Rq, isize>::new();
	let mut regqueue = std::collections::VecDeque::from([Rq::R12, Rq::R13, Rq::R14, Rq::R15, Rq::RBP]);

	let mut load_tape = |t, jit: &mut Assembler<X64Relocation>| {
		if let Some(r) = tape2reg.get(&t) {
			return *r;
		}
		let r = regqueue.pop_front().unwrap();
		regqueue.push_back(r);
		if let Some(i) = reg2tape.remove(&r) {
			tape2reg.remove(&i).unwrap();
			dynasm!(jit ; mov [rbx + (i * 8).try_into().unwrap()], Rq(r.code()));
		}
		dynasm!(jit ; mov Rq(r.code()), QWORD [rbx + (t * 8).try_into().unwrap()]);
		tape2reg.insert(t, r);
		reg2tape.insert(r, t);
		r
	};
	let call_print = |jit: &mut Assembler<X64Relocation>, reg: Rq| {
		dynasm!(jit
			; mov rdi, Rq(reg.code())
			; mov rax, QWORD print as _
			; call rax
		);
	};

	let mut last_reg = None;

	for (i, (op, &lbl)) in ops.into_iter().zip(labels.iter()).enumerate() {
		match op {
			Op::SetV(a, b) => {
				dynasm!(jit ; =>lbl);
				let r = load_tape(a, &mut jit);
				if let Ok(b) = i8::try_from(b) {
					dynasm!(jit ; add Rq(r.code()), BYTE b);
				} else if let Ok(b) = i32::try_from(b) {
					dynasm!(jit ; add Rq(r.code()), DWORD b);
				} else {
					todo!();
				}
				dynasm!(jit ; mov [rbx + (a * 8).try_into().unwrap()], Rq(r.code()));
				(a == 0).then(|| call_print(&mut jit, r));
				last_reg = Some(r);
			}
			Op::SetA(a, b) => {
				dynasm!(jit ; =>lbl);
				let r = load_tape(a, &mut jit);
				dynasm!(jit ; add Rq(r.code()), [rbx + (b * 8).try_into().unwrap()]);
				dynasm!(jit ; mov [rbx + (a * 8).try_into().unwrap()], Rq(r.code()));
				(a == 0).then(|| call_print(&mut jit, r));
				last_reg = Some(r);
			}
			Op::JmpV(a, b) => {
				let a = load_tape(a, &mut jit);
				dynasm!(jit
					; =>lbl
					; cmp Rq(a.code()), Rq(last_reg.unwrap().code())
					; jne =>labels[i - b as usize - 2]
				);
			}
			Op::JmpA(_a, _b) => {
				todo!()
			}
		}
	}
	dynasm!(jit
		; pop r12
		; pop r13
		; pop r14
		; pop r15
		; pop rbp
		; pop rbx
		; ret
	);
	let f = jit.finalize().unwrap();
	std::fs::write("/tmp/jit2.out", &f[..]).unwrap();
	let f: extern "C" fn(*mut isize) =
		unsafe { core::mem::transmute(f.ptr(dynasmrt::AssemblyOffset(0))) };
	let t = Instant::now();
	f([0; 0x10000].as_mut_ptr());
	eprintln!("{:?}", Instant::now() - t);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
	const USAGE: &str = "usage: <interpreter|jit|jit2> <file>";
	let mut args = std::env::args().skip(1);
	let mode = args.next().ok_or(USAGE)?;
	let f = args.next().ok_or(USAGE)?;
	let f = std::fs::read(f)?;
	let f = parse(f);
	match &*mode {
		"interpreter" => run(f),
		"jit" => jit(f),
		"jit2" => jit2(f),
		_ => Err(USAGE)?,
	}
	Ok(())
}
