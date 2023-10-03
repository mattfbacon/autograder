import cbor2
import resource
import subprocess
import sys
from typing import Callable, Any, NoReturn


# Common configuration.

COMPILATION_TIMEOUT = 5
VERSION_TIMEOUT = 10


# Utilities.

def write(path: str, contents: bytes) -> None:
	with open(path, 'wb') as f:
		f.write(contents)

def parse_tests(raw: str) -> list[tuple[str, str]]:
	return [tuple(s.strip() for s in case.split('\n--\n', 1)) for case in raw.split('\n===\n')]

def write_output(output: Any) -> NoReturn:
	cbor2.dump(output, sys.stdout.buffer)
	exit(0)

# Return early and terminate the process if the process fails.
def compile_run(args: list[str]) -> None:
	output = subprocess.run(args, timeout=COMPILATION_TIMEOUT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, encoding='utf8')
	if output.returncode != 0:
		reason = f'While running {repr(args)}:\n\n' + output.stdout
		write_output({ 'InvalidProgram': reason })


# Language-specific code.
# - Compile functions take in code as text and return a file path which will be forwarded to the run function.
# - Run functions take in the path returned from `compile` and return arguments for the process to spawn.
# - Version functions just return the version as a string, as well as the compilation arguments if they are deemed important.

PYTHON = '/usr/bin/python3'

def compile_python3(code: str) -> str:
	path = 'source.py'
	write(path, code.encode())
	compile_run([PYTHON, '-m', 'py_compile', path])
	return path

def run_python3(path: str) -> list[str]:
	return [PYTHON, path]

def version_python3() -> str:
	return subprocess.run([PYTHON, '--version'], encoding='utf8', capture_output=True, check=True).stdout


CC = 'gcc'
CXX = 'g++'
CCFLAGS = ['-O2', '-march=native', '-pipe', '-w', '-fmax-errors=3']
CCFLAGS_AFTER = ['-lm']
CFLAGS = ['-std=gnu2x']
CXXFLAGS = ['-std=gnu++23']

def compile_c(code: str) -> str:
	path = 'source.c'
	out = './source'
	write(path, code.encode())
	compile_run([CC, *CCFLAGS, *CFLAGS, path, *CCFLAGS_AFTER, '-o', out])
	return out

def run_c(path: str) -> list[str]:
	return [path]

def version_c() -> str:
	return subprocess.run([CC, '--version'], encoding='utf8', capture_output=True, check=True).stdout + f'Cmdline: {CC} {" ".join(CCFLAGS + CCFLAGS_AFTER + CFLAGS)}\n'


def compile_cpp(code: str) -> str:
	path = 'source.cpp'
	out = './source'
	write(path, code.encode())
	compile_run([CXX, *CCFLAGS, *CXXFLAGS, path, *CCFLAGS_AFTER, '-o', out])
	return out

def run_cpp(path: str) -> list[str]:
	return [path]

def version_cpp() -> str:
	return subprocess.run([CXX, '--version'], encoding='utf8', capture_output=True, check=True).stdout + f'Cmdline: {CXX} {" ".join(CCFLAGS + CCFLAGS_AFTER + CXXFLAGS)}\n'


JAVA = 'java'
JAVAC = 'javac'
JAR = 'jar'

def compile_java(code: str) -> str:
	main_class = 'Main'
	path = f'{main_class}.java'
	class_dir = 'classes'
	jar = 'jar.jar'
	write(path, code.encode())
	compile_run([JAVAC, '-d', class_dir, '-encoding', 'UTF8', path])
	compile_run([JAR, 'cfe', jar, main_class, '-C', class_dir, '.'])
	return jar

def run_java(path: str) -> list[str]:
	return [JAVA, '-jar', path]

def version_java() -> str:
	return subprocess.run([JAVA, '--version'], encoding='utf8', capture_output=True, check=True).stdout + '\n' + subprocess.run([JAVAC, '--version'], encoding='utf8', capture_output=True, check=True).stdout


RUSTC = 'rustc'
RUSTC_ARGS = ['--crate-name=program', '--crate-type=bin', '--edition=2021', '-Copt-level=3', '-Ctarget-cpu=native']

def compile_rust(code: str) -> str:
	path = 'source.rs'
	out = './source'
	write(path, code.encode())
	compile_run([RUSTC, *RUSTC_ARGS, path, '-o', out])
	return out

def run_rust(path: str) -> list[str]:
	return [path]

def version_rust() -> str:
	return subprocess.run([RUSTC, '--version'], encoding='utf8', capture_output=True).stdout + '\n' + f'Cmdline: {RUSTC} {" ".join(RUSTC_ARGS)}'


# Main actions.

LANGUAGE_FUNCS: list[tuple[Callable[[str], str], Callable[[str], list[str]], Callable[[], str]]] = [
	(compile_python3, run_python3, version_python3),
	(compile_c, run_c, version_c),
	(compile_cpp, run_cpp, version_cpp),
	(compile_java, run_java, version_java),
	(compile_rust, run_rust, version_rust),
]

def do_test(command):
	(compile, run, _version) = LANGUAGE_FUNCS[command['language']]

	compiled_path = compile(command['code'])
	args = run(compiled_path)

	memory_limit = command['memory_limit'] * 1_000_000
	timeout = command['time_limit'] / 1000

	tests = parse_tests(command['tests'])
	passes = []

	for (input, expected_output) in tests:
		try:
			output = subprocess.run(args, timeout=timeout, input=input, encoding='utf8', capture_output=True, preexec_fn=lambda: resource.setrlimit(resource.RLIMIT_RSS, (memory_limit, memory_limit)))
			if output.returncode != 0:
				pass_result = 'RuntimeError'
			elif output.stdout.strip() == expected_output:
				pass_result = 'Correct'
			else:
				pass_result = 'Wrong'
		except subprocess.TimeoutExpired:
			pass_result = 'Timeout'

		passes.append(pass_result)

	return { 'Ok': passes }

def do_versions(_command):
	return [version() for (_compile, _run, version) in LANGUAGE_FUNCS]


# Driver code.

with open('/input/command', 'rb') as command_file:
	command = cbor2.load(command_file)

COMMANDS = { 'Test': do_test, 'Versions': do_versions }

response = COMMANDS[command['command']](command)
write_output(response)
