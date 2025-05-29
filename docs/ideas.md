## 设计思路


### 节点变量

起因：现在节点都是用第一个字符串参数作为ID，其他节点用ID来引用。这样书写麻烦，而且容易出错。如果改成类似变量的形式，可以借助解释器的变量名称检查，避免出错。而且节点加上变量之后，更像是函数形式了，也更好看一些。

现有的节点语法

```rust
// 声明
lib("my_math") {
    at: "lib/my_math"
}

// 调用
app("demo") {
    links: ["my_math"]
    // 或
    link("my_math") {}
}

```

新的节点声明语法

```rust
// 声明
lib my_math {
    at: "lib/my_math"
}

// 调用
app demo {
    links: [my_math]
    // 或
    link(my_math) {}
}
```

### 更强类型的包结构和节点结构

现在的`pac.at`都是一堆散乱的AutoConfig节点，引用的方式也是用`dep`节点来表示。

考虑到现在`dep`节点已经分成了`dep`（负责下载）和`link`（负责实例化）两个过程，
整个包的导入也不再是一次性的静态过程，
我打算把`pac`包做成类似`module`的语法结构；而包内的编译目标（如`lib`、`device`等）则做成类似函数的形式。

这样做的话，导入一个包，和标准程序里`use`一个模块的过程几乎一样了，因此可以考虑直接复用`use`语法。
而引用包里的一个目标，用法也类似于调用模块里的一个函数（或是实例化模块中的一个结构体），因此可以把`link`节点做成节点实例化的形式。


### 节点模型（Schema）

现在的目标节点都是自由节点，参数、属性和子节点都没有可以声明的约束。

考虑引入模式Schema来约束节点的生成。这个概念类比于`struct`和数据实例的关系。

节点 | 类型 | 函数
--- | --- | ---
Schema | 类型声明 | 函数声明
Node（节点实例） | 对象（类型实例化） | 返回值（函数调用）

Schema的例子：

```rust
node src (path str) {}

node dir (id sym) {
    at str
}

// 声明一个tag名称为lib的节点类型
node lib (id sym) {
    // 属性声明
    at str

    // 子节点声明
    kids: [src, dir]
}
```

节点实例化的例子：

```rust
lib my_math {
    at: "lib/my_math"

    dir("src") {}

    dirs: ["include", "source"]
}
```

注意，这里的`dir`节点和`dirs`数组都会转化为`my_math`的子目录节点。

### 半实例化

由于某些节点需要多个实例，但每个实例又有公共的内容，因此我们需要半实例化的节点，先把公共部分数据准备好，再在需要的地方进一步完成实例化。

```rust
lib my_math? (use_float: true) {
    at: "lib/my_math"
    dirs: ["include", "source"]
    if use_float {
        src("float") {}
    }
}
```

例如，这里的`my_math`后面有个问号，表示这个实例还没有完全生成。

当我们需要使用实际的`my_math`节点时，需要再一次用`n(){}`的语法实例化它：

```rust
app low_end {
    link {
        // 这里use_float参数为false
        my_math(use_float: false) {}
    }
}

app high_end {
    link {
        // 这里use_float的参数是默认值true
        my_math() {}
    }
}
```

上面两个不同的`app`中，`high_end`给定了参数`use_float`为`true`，
而`low_end`这个值是`false`，
因此`high_end`比`low_end`多包含一个`"float"`子目录。


### link

节点声明之后，可以用`link`来指向它。


```rust
app hello {
    link(my_math) {

    }
}

```
