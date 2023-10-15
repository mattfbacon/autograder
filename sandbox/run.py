import cbor2
import errno
import importlib.util
import inspect
import os
import resource
import subprocess
import sys
import time
from typing import Callable, Any, NoReturn


# Common configuration.

COMPILATION_TIMEOUT = 5
VERSION_TIMEOUT = 10
COMMAND_PATH = 'command'


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
def compile_run(args: list[str], extra: str = '') -> None:
	output = subprocess.run(args, timeout=COMPILATION_TIMEOUT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, encoding='utf8')
	if output.returncode != 0:
		reason = f'While running {repr(args)}:\n\n' + output.stdout + extra
		write_output({ 'InvalidProgram': reason })

# <https://stackoverflow.com/a/53080237>
def module_from_str(name: str, source: str) -> Any:
	spec = importlib.util.spec_from_loader(name, loader=None)
	assert spec is not None
	module = importlib.util.module_from_spec(spec)
	exec(source, module.__dict__)
	return module


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
JAVA_EXTRA = "\nNote that the program's class should be named Main.\n"

def compile_java(code: str) -> str:
	main_class = 'Main'
	path = f'{main_class}.java'
	class_dir = 'classes'
	jar = 'jar.jar'
	write(path, code.encode())
	compile_run([JAVAC, '-d', class_dir, '-encoding', 'UTF8', path], extra=JAVA_EXTRA)
	compile_run([JAR, 'cfe', jar, main_class, '-C', class_dir, '.'], extra=JAVA_EXTRA)
	return jar

def run_java(path: str) -> list[str]:
	return [JAVA, '-jar', path]

def version_java() -> str:
	return subprocess.run([JAVA, '--version'], encoding='utf8', capture_output=True, check=True).stdout + '\n' + subprocess.run([JAVAC, '--version'], encoding='utf8', capture_output=True, check=True).stdout + JAVA_EXTRA


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

	judger: Callable[[int, str, str, str], bool] = module_from_str('judger', j).judge if (j := command.get('custom_judger')) is not None and len(j) > 0 else lambda _i, _input_case, expected_output, actual_output: expected_output == actual_output

	compiled_path = compile(command['code'])
	args = run(compiled_path)

	timeout = command['time_limit'] / 1000

	tests = parse_tests(command['tests'])
	passes = []

	for i, (input, expected_output) in enumerate(tests):
		input = input.strip() + '\n'
		expected_output = expected_output.strip()

		before = time.perf_counter_ns()

		try:
			result = subprocess.run(args, input=input, timeout=timeout, encoding='utf-8', capture_output=True)
			after = time.perf_counter_ns()

			if result.returncode != 0:
				pass_result = 'RuntimeError'
			else:
				try:
					ret = judger(i, input, expected_output, result.stdout.strip())
					pass_result = 'Correct' if ret else 'Wrong'
				except Exception as e:
					write_output({ 'InvalidProgram': f'judger raised an exception: {e}' })
		except subprocess.TimeoutExpired:
			pass_result = 'TimeLimitExceeded'
			after = time.perf_counter_ns()

		elapsed_time = (after - before) // 1_000_000

		passes.append({ 'kind': pass_result, 'time': elapsed_time })

	return { 'Ok': passes }

def do_validate_judger(command):
	judger = command['judger']
	try:
		judge = module_from_str('judger', judger).judge
		inspect.signature(judge)
	except:
		return { 'Err': str(sys.exc_info()[1]) }
	return { 'Ok': None }


def do_versions(_command):
	return [version() for (_compile, _run, version) in LANGUAGE_FUNCS]


# Driver code.

with open(COMMAND_PATH, 'rb') as command_file:
	command = cbor2.load(command_file)
os.remove(COMMAND_PATH)

COMMANDS = { 'Test': do_test, 'Versions': do_versions, 'ValidateJudger': do_validate_judger }

response = COMMANDS[command['command']](command)
write_output(response)
