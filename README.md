## AutoMan

AutoMan（Auto Manager）是Auto/C语言的工程管理工具，功能包括：

- 编译管理：将Auto编译为C；将C编译为二进制程序
- C语言工程管理：可以生成CMake、IAR、GHS和CCS等编译工程
- 包管理：以`pac`为单位组织代码库
- 依赖管理：自动下载依赖库
- 资源管理：配置并下载相关资源，如芯片的库文件和配置等。
- 测试管理：自动运行测试用例

AutoMan的配置语言是`Auto`语言的`Config`格式。

## 简介

AutoMan主要有如下几类子命令：

1. 新建工程：
  - `am app`：新建Auto语言的可执行工程
  - `am lib`：新建Auto语言的代码库工程
  - `am capp`：新建C语言的可执行工程
  - `am clib`：新建C语言的代码库工程
2. 构建工程：
  - `am scan`：扫描工程，获取依赖库。
  - `am build`：构建工程，缩写是`am b`。`am b`会自动运行`am scan`
  - `am run`：运行工程，缩写是`am r`
  - `am open`：打开工程对应IDE，缩写是`am o`
  - `am port`：切换当前的移植目标
3. 依赖库相关：
  - `am deps`：查看所有可用的依赖库
  - `am devices`：查看所有可用的设备资源
  - `am pull`：更新所有依赖库，包括依赖索引
4. 辅助命令：
  - `am help`：显示帮助信息
  - `am upgrade`：更新AutoMan到最新版本


## pac.at

Automan的工程管理信息都存放在`pac.at`文件中，`pac`即`package`，`at`则是`auto`的缩写，表示这是一个Auto语言的代码文件。

### AutoConfig结构

`pac.at`的格式是`AutoConfig`，类似于JSON/XML。它的基本结构是这样的：

```rust
// 属性对
key1: 15
key2: false
key3: ["a", "b", "c", "d"]
key4: {
    key5: value5
    key6: []
}

// 节点
app("hello") {
    // 节点的属性
    at: "apps/hello"
    recurse: true

    // 子节点
    dir("motor") {
        dir("bldc") {
            // ....
        }
    }
}
```

总体而言，AutoConfig的第一层由属性和多个节点组成，而节点内部又可以包含属性和多个子节点，依次递归，可以形成一个树状结构。

除了静态的属性和节点，`pac.at`也支持Auto语言的脚本功能，包括：

```rust
// 变量声明
var rh = "RH850"

// 条件语句
if port == rh {
    device("Renesas-RH850") {}
} else {
    // ...
}
```

未来还会添加其他有用的动态语言功能，如函数调用等。

## Automan的内置节点

与CMake类似，Automan将可以编译的对象称为`编译目标`（build target）。
Automan支持如下几类编译目标：

- 可执行文件：app
- 静态库文件：lib
- 动态库文件：dll (暂未实现)
- 芯片资源包：device
- 依赖库：dep
- 单纯代码包：bag

Automan提供了几个内置节点，用来声明工程中编译对象的属性。

```rust
app("hello") {
    at: "source/app"
    dir("motor") {
        dir("bldc") {}
    }
    dir("can") {}
    dir("diag") {}
}
```

上面的节点是一个可执行目标，它最终会编译成为一个可执行文件，名称是`hello.exe`。

默认情况下，Automan会到`$PROJECT_ROOT/apps/hello`目录下去寻找`hello`目标的代码，
但我们也可以用`at`属性来指定目录。
上面的例子中，`hello.exe`的源码放在`$PROJECT_ROOT/source/app`中。

`dir`节点表示目标节点内部的子目录，例如上面的`dir("bldc"){}`，对应的就是`$PROJECT_ROOT/source/app/motor/bldc`。
默认情况下，Automan会扫描目标或`dir`中的几个目录：

- 当前目录
- src目录
- inc目录
- include目录

如果这些目录中发现有合法的源码文件，如`.c`，`.h`文件，Automan会收集相关信息，并用于生成最终的构建脚本。

多个节点如果只需要提供名称，其他情况下都用默认值的话，也可以直接用数组：

```rust
app("hello") {
  dirs: ["a", "b", "c"]
}
```

相当于

```rust
app("hello") {
    dir("a") {}
    dir("b") {}
    dir("c") {}
}

`lib`节点和`app`节点类似，都是独立的编译目标。区别是`app`会被编译成可执行文件，而`lib`会被编译成静态库。
在IAR或GHS中，整个工程是一个统一的编译目标，此时app以及它对应的所有依赖的lib/dep/device都会被当做普通目录合并到一起。

`dep`节点和普通的`lib`节点类似，都是编译库。区别在于`lib`是项目内部定义的库，而`dep`是从网络上下载的库。

`dep`节点定义方式如下：

```rust
dep("log", version:"0.0.2") {
    at: "source/lib/log"
    // 其他参数
}
```

这里如果不声明version字段，则会默认使用最新版，即git仓库中的master:HEAD。
如果不指定`at`属性，则下载的`log`库会默认放在`$PROJECT_ROOT/deps/log`目录中。

因此，最简单的依赖声明方式如下：

```rust
dep("log") {}
```

注意，`dep`目标只负责下载到本地，但是如果本地的`app`或`lib`需要使用它（即链接它）时，需要用`link`节点来指定：

```rust

app("hello") {
    link("log") {}
}
```

此时，`log`工程才会被认为是`hello`目标的链接库，一起编译。

`device`和`dep`概念相似，唯一的区别是`device`用作拉取硬件相关的资源，考虑到架构差异，Automan是用不同的索引管理的。

`bag`是一个特殊的代码包，本身并不是独立的编译目标，而是用来组织一些散落在目录结构中各处的代码。`app`和`lib`可以用`link`来依赖它们。有`bag`节点，我们可以更好地管理多个`app`公用的代码目录。

## 示例：新建工程

新建工程有如下几个命令：

- `am app`：新建Auto语言的可执行工程
- `am lib`：新建Auto语言的代码库工程
- `am capp`：新建C语言的可执行工程
- `am clib`：新建C语言的代码库工程

要建立Auto语言的可执行工程，直接使用`am app`命令。

```bash
am app hello
```

会得到如下目录结构：

```
hello
├── pac.at
└── hello.at
```

- `pac.at`是工程的配置文件，`pac`是`package`的缩写，表示一个代码包。
- `hello.at`是Auto语言的代码文件。

`hello.at`内容如下：

```rust
fn main {
  print("Hello, World!")
}
```

编译和运行：

```bash
> am build
> am run
Hello, World!
```

如果想要生成C语言的工程，则使用`am capp`命令：

```bash
am capp hello
```

得到如下目录结构：

```
hello
├── pac.at
└── hello.c
```

C语言工程的编译和运行也是`build`和`run`命令。

如果要新建代码库的工程，则使用`am lib`命令。

```bash
am lib mymath
```

得到如下目录结构：

```
mymath
├── pac.at
└── mymath.at
```

mymath.at内容如下：

```rust
fn add(a int, b int) int {
  a + b
}
```

这个工程可以用作依赖库，给其他的Auto语言工程使用。

如果想要C语言的库工程，使用`am clib`命令：

```bash
am clib mymath
```

得到如下目录结构：

```
mymath
├── pac.at
├── mymath.h
└── mymath.c
```

这里的`mymath.h`是头文件，`mymath.c`是源文件。

## 依赖管理


例如，假设我们已经建立了一个`mymath`的工程，并且上传到了repo中（具体如何上传，后面再介绍）。

现在我们新建一个`hello`工程，其`pac.at`内容如下：

```rust
name: "hello"
version: "0.0.1"

app("hello") {}
```

AutoMan的依赖管理是基于Git的，配置的方式是在`pac.at`中添加`dep`节点。
要添加对`mymath`的依赖，可以在`pac.at`中添加如下内容：

```rust
name: "hello"
version: "0.0.1"

// 添加对mymath的依赖
dep("mymath") {
    // 版本也可以不指定，会自动使用最新版
    version: "0.0.1"
}

app("hello") {
    // 把mymath链接到hello包
    links: ["mymath"]
}
```

此时，再运行`am build`时，会自动下载`mymath`库，得到如下结构：

```
hello
├── pac.at
├── hello.at
└── deps
    └── mymath
        ├── pac.at
        ├── mymath.h
        └── mymath.c

```

此时修改`hello.at`源码，就可以使用`mymath`库了。

```rust
use mymath:add

fn main {
    print(add(1, 2))
}
```

如果是C语言的`app`项目，过程也是类似，只不过引用库时需要使用`#include`语句。

```c
#include "mymath.h"

int main(void) {
    printf("%d\n", mymath_add(1, 2));
    return 0;
}
```

注意，在C语言中，函数名是包含了包名前缀`mymath_`的。

## 编译流程

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
- `lib`：代码库
- `dep`：依赖库，它本身是一个`lib`代码包
- `bag`：普通代码包，它并不是独立的构建单元，但是可以用来组织代码
- `test`：测试库
- `doc`：文档

一个代码包`pac`可以包含多个`target`：

```rust
name: "hello"
version: "0.0.1"

app("hello") {}
app("hi") {}
lib("mymath") {}
lib("mystr") {}
```

上述的`pac.at`对应于如下的代码结构：

```
hello
├── pac.at
├── hello.at
├── apps
│   └── hi.at
└── libs
    ├── mymath.at
    └── mystr.at
```

注意，由于目标`hello`名字和整个代码包相同，说明它是项目的主目标，
因此放在项目的根目录中，而不是放在`apps`或`libs`目录中。

如果是C文件，则会生成不同的子目录：

```
hello
├── pac.at
├── hello.c
├── apps
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

注意，这里的`hello`也是主目标，因此它的原文件放在根目录中，而不是放在`apps`或`libs`目录中。

`hello`和`hi`都是应用程序文件，因此调用`am build`时，会生成两个不同的可执行文件：

```
build/Debug/hello.exe
build/Debug/hi.exe
```

直接调用`am run`时，系统会弹出提示，要求用户选择执行哪一个：

```
shell> am run

Please select a target to run：

1. hello (main target)
2. hi

If you press Enter, the main target `hello` will be selected.

> 1
Hello, World!
```

选择`1`，则执行`hello.exe`；选择`2`，则执行`hi.exe`。
如果直接回车，则会选择主目标，即`hello.exe`。

## Auto和C的混合工程

在AutoMan的工程中，Auto和C的`target`是可以混合使用的，
这是因为AutoMan编译Auto和C的过程是相互独立的：

1. 首先扫描所有`target`，检查是否包含Auto源码（即`*.at`文件）。
2. 将所有的`*.at`文件转译为对应的C代码，即`*.h`和`*.c`文件。
3. 然后再扫描一遍，这次只扫描C的源码（即`*.c`文件和`*.h`文件）。
4. 最后，编译C的源码。

作为类比，Auto和C的关系与TypeScript和JavaScript的关系类似。

Auto和TypeScript都有一道“预编译”的过程。
预编译之后，工程可以当作普通的纯C或纯JavaScript工程来处理。

## 测试

AutoMan的测试功能是基于Auto语言来实现的。
AutoMan的测试功能包括单元测试、黑盒测试与网络测试。

当前版本只实现了简单的单元测试，未来会添加黑盒测试和网络测试。

AutoMan的单元测试是基于代码包和Auto语言的模块来组织的。

对于一个代码包，例如`mymath`，我们可以添加一个`test`的`target`：

```bash
am addtest mymath
```

注意，由于`test`这种编译目标只支持Auto语言，所以不能使用`amc`命令来添加。

上面的命令会生成一个针对`mymath`包的单元测试文件`test_mymath.at`，
内容如下：

```rust
// test_mymath.at
fn test_add {
  assert(false)
}
```

默认情况下，`addtest`命令会给`mymath`包的每一个公开的函数（定义在`mymath.at`）
生成一个测试用例。

测试用例的函数名以`test_`开头。默认是无法通过的。

```bash
bash> am test
Testing package mymath...

case 1: test_add: failed

Failed: 0/1 tests passed.
```

我们修改上面的`test_add`用例：

```rust
fn test_add {
    assert_eq(5, add(2, 3))
}
```

此时再次调用测试命令：

```bash
bash> am test
Testing package mymath...

case 1: test_add: passed

Passed: 1/1 tests passed.
```

## 文档

TODO: 此功能暂时未实现


## 依赖管理

除了组织本地工程，AutoMan还实现了基于Git的依赖管理和自动下载功能。

AutoMan的依赖管理类似于Rust的Cargo/Crates:

1. 有一个中心的资源库，统一存放所有依赖包的信息，以及查询的索引。类似于crates.io
2. 每个独立的代码包，都有自己的名称。
3. 代码包也有基于版本号的管理。TODO：暂时未实现。

与Cargo/Crates不同的是：

1. 为了避免代码包重名，AutoMan还增加了一个`vendor`的概念，不同的`vendor`可以提供相同名称的代码包。
2. 在引用依赖包的时候，如果没有重名，直接用代码包名称即可；如果有重名，系统会提示添加`vendor`名称来区分。

`vendor/pac`的双层结构和Github的`user/project`结构类似。

下面是一个代码包的示例，例如，我们新建的`mymath`代码包，`vendor`是`soutek`：

```rust
name: "mymath"
vendor: "soutek"
version: "0.0.1"

lib("mymath") {}
```

如果新建一个应用工程`hallo`，想要引入`mymath`包，则可以使用如下命令：

```bash
bash> am new hallo
bash> cd hello
bash> am dep mymath
```

如果没有重名，那么AutoMan会直接找到`soutek/mymath`库，并自动下载；
如果有其他`vendor`也上传了`mymath`库，则会提示用户：

```bash
bash> am dep mymath
....
Found 2 pacs with the name `mymath`:

1. soutek/mymath
2. gigi/mymath

Please select a number, or press Enter for the first one:

> 1
`soutek/mymath` selected.

Please choose a version, or press Enter for the latest:

1. 0.1.5
2. 0.1.4
3. 0.1.3
...

> 1

... `soutek/mymath:0.1.5` selected.
... Downloading dependency...
... Done.
```

如果已经知道需要的`vendor`和`version`，可以直接指定：

```bash
bash> am dep soutek/mymath:0.1.5
```

注意，`vendor`和`pac`之间用`/`分隔，而`pac`和`version`之间用`:`分隔

也可以忽略`vendor`，只指定`pac`和`version`：

```bash
bash> am dep mymath:0.1.5
```

上述命令的效果，相当于先修改`pac.at`，再执行`am update`：

添加了依赖的`pac.at`如下：

```rust
name: "hallo"
version: "0.0.1"

dep("mymath", vendor: "soutek", version: "0.1.5")

app("hallo") {}
```

注意，依赖虽然已经下载过来，但是实际上我们的`hallo`应用并没有和它关联起来。

我们需要调用`link`命令把它们关联起来：

```bash
bash> am link hallo mymath
```

如果有多个依赖，可以一起写：

```bash
bash> am link hallo mymath mystr myio
```

`link`命令的效果相当于修改`pac.at`：

```rust
app("hello") {
    link("mymath") {}
    link("mystr") {}
    link("myio") {}
}
```

注意：`dep`和`link`命令只会修改工程的属性，但并不会修改代码本身。
因此，即使加上了对`mymath`的依赖，我们仍然需要在代码中自己写下`use`或`include`语句：

```rust
// Auto文件
use mymath;
use mystr;
use myio;
```

或者C文件：

```c
// C文件
#include "mymath.h"
#include "mystr.h"
#include "myio.h"
```
