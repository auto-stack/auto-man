## AutoMan

AutoMan（Auto Manager）是Auto/C语言的混合工程管理工具，功能包括：

- Auto语言工程管理：Auto语言的代码编译成C语言
- C语言工程管理：可以生成CMake、Ninja、IAR、GHS和CCS等工程。
- 包管理：以`pac`为单位，组织工程的结构
- 依赖管理：自动下载依赖库
- 资源管理：配置并下载工程资源，如芯片相关的库文件、文档等。
- 测试管理：自动运行测试用例

AutoMan的配置文件是`AutoConfig`格式。

## 简单示例：纯C语言工程

新建工程:

```bash
amc new hello
```

`amc`命令是`automan -c`命令的缩写，其中`new`表示新建一个纯C语言的工程。
这个命令会生成一个`hello`工程，工程目录结构如下：

```
hello
├── pac.at
├── apps
    └── hello
        └── hello.c
```

这里:

- `pac.at`是工程的配置文件，`pac`是`package`的缩写，表示一个代码包。
- `apps`目录是应用程序的代码目录；相对应的，`libs`目录则表示代码库的目录。
- `hello/hello.c`是`hello`包所对应的可执行程序。

`pac.at`格式是AutoConfig，默认内容如下：

```rust
name: "hello"
version: "0.0.1"

app("hello") {}
```

`hello.c`内容如下：

```c
#include <stdio.h>

int main(void) {
    printf("Hello, World!\n");
    return 0;
}
```

如果需要新建一个代码库工程，而不是可执行应用程序，可以使用`amc newlib`命令。

```bash
amc newlib mymath
```

这个命令会生成如下的目录结构：

```
mymath
├── pac.at
└── libs
    └── mymath
        ├── mymath.h
        └── mymath.c
```

此时`pac.at`内容如下：

```rust
name: "mymath"
version: "0.0.1"

lib("mymath") {}
```

`mymath/mymath.h`内容如下：

```c
#ifndef MYMATH_H
#define MYMATH_H

int add(int a, int b);

#endif // MYMATH_H
```

`mymath/mymath.c`内容如下：

```c
#include "mymath.h"

int add(int a, int b) {
    return a + b;
}
```

## 简单示例：Auto语言工程

要建立Auto语言的工程，直接使用`am new`和`am newlib`命令即可。

```bash
am new hello
```

会得到如下目录结构：

```
hello
├── pac.at
└── apps
    └── hello
        └── hello.at
```

和纯C工程唯一的区别，是生成的`hello.at`文件是Auto语言的代码文件。

`hello.at`内容如下：

```rust
fn main {
  print("Hello, World!")
}
```

如果要新建代码库的工程，则使用`am newlib`命令。

```bash
amc newlib mymath
```

得到如下目录结构：

```
mymath
├── pac.at
└── libs
    └── mymath
        └── mymath.at
```

mymath.at内容如下：

```rust
fn add(a int, b int) int {
  a + b
}
```

用`am build`编译工程时，会先把Auto语言的`*.at`文件转译成对应的C语言的`*.h`和`*.c`文件。
然后再按照纯C语言的工程进行编译。

`am build`会把`mymath.at`转译成和前面的纯C工程里生成的`mymath.h`和`mymath.c`文件。

在Windows下，Automan默认会生成基于CMake的Visual Studio工程。这是由于CMake对Visual Studio的支持很好，因此我就没有单独制作专门生成Visual Studio工程的逻辑了。

在Windows下，`am build`会执行如下操作：

1. 生成一个`CMakeLists.txt`文件
2. 调用`cmake -A win32 -B build`，在`build`目录下生成Visual Studio工程
3. 调用`cmake --build build`，在`build`目录下编译工程

此时会在`build/Debug/`目录下生成`hello.exe`可执行文件或`mymath.lib`库文件。

对于`app`包，我们也可以直接调用`am run`来运行程序。它相当于：

1. 调用`am build`编译工程
2. 调用`build/Debug/hello.exe`来运行程序

## 多个`target`

上面的示例中，我们新建了一个`hello`的应用程序，以及一个叫`mymath`的代码库。

在AutoMan中，应用程序(`app`)和代码库(`lib`)，统一被称为`编译目标`（简称`target`）。

除了可执行的应用程序和代码库，AutoMan还支持如下类型的`target`：

- `app`：可执行的应用程序
- `lib`：静态代码库
- `dylib`：动态链接库，即`Dyamic Library`（在Windows下，被称为`DLL`；在Linux下，被称为`so`）
- `obj`：目标文件，即`Object File`。这是一种中间文件格式，可以用来拼接成上述三种文件。
- `doc`：文档

注意，`dylib`其实就是`lib(<name>, kind = "dylib")`的缩写；
而`obj`其实就是`lib(<name>, kind = "obj")`的缩写。

一个代码包`pac`可以包含多个`target`：

```rust
name: "hello"
version: "0.0.1"

app("hello") {}
app("hi") {}
lib("mymath") {}
dylib("mystr") {}
```

上述的`pac.at`对应于如下的代码结构：

```
hello
├── pac.at
├── apps
│   ├── hello.at
│   └── hi.at
└── libs
    ├── mymath.at
    └── mystr.at
```

注意，`lib`、`dylib`和`obj`都放在`lib`目录下，因为他们都是不同形式的库文件。

如果是C文件，则会生成不同的子目录：

```
hello
├── pac.at
├── apps
│   ├── hello
│   │   └── hello.c
│   └── hi
│       └── hi.c
└── libs
    ├── mymath
    │   ├── mymath.h
    │   └── mymath.c
    └── mystr
        ├── mystr.h
        └── mystr.c
```

`hello`和`hi`都是应用程序文件，因此调用`am build`时，会生成两个不同的可执行文件：

```
build/Debug/hello.exe
build/Debug/hi.exe
```

直接调用`am run`时，系统会弹出提示，要求用户选择执行哪一个：

```
shell> am run

Please select a target to run：

1. hello
2. hi

If you press Enter, the first target will be run.

> 1
Hello, World!
```

选择`1`，则执行`hello.exe`；选择`2`，则执行`hi.exe`。
如果直接回车，则会选择第一个执行目标，即`hello.exe`。


