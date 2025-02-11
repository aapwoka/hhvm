# Adapters

<!-- https://www.internalfb.com/intern/wiki/Thrift/Thrift_Guide/Adapter/?noredirect -->

## Overview

Thrift Adapters allow the native type used in Thrift code gen, to be customized without writing a lot of code or digging into Thrift internals. This decouples changes in Thrift from the functionality of these custom objects, so they can be maintained directly by individual teams, writing their own libraries to hook into Thrift to achieve their desired API.

To use Adapter, we can apply structured annotation `@{lang}.Adapter{name = "..."}` to a field or a typedef, with the name of adapter class. Adapter provides `from_thrift` and `to_thrift` APIs. Here is an example in C++ (The APIs are similar in other languages)

```cpp
struct Adapter {
  static AdaptedType fromThrift(DefaultType);
  static DefaultType toThrift(const AdaptedType&);
};
```
This completely replaces the underlying type of a thrift from default type to adapted type and uses the specified adapter to convert to and from the underlying Thrift type during (de)serialization.

## Terminologies

* **Default type** - A language specific type that has built-in support in the Thrift runtime without customization. For example, in C++ for `list<i32>` the default type is `std::vector<std::int32_t>>`.
* **Adapted type** - A custom type that is convertible to/from a default type when used with the Thrift runtime.
* **Type adapter** — allows user to change any default type to custom type.
   * If you want to change the type inside a container (e.g. `list<MyCustomType>`), you should use type adapter.
* **Field adapter** — allows user to change type of thrift field to custom type
   * If your adapter needs to access field id, you should use field adapter (or field wrapper in Hack and Java).

## C++

C++ adapter can be enabled via `cpp.Adapter` annotation. e.g.

```cpp
// In C++ Header
struct BitsetAdapter {
  static std::bitset<32> fromThrift(std::uint32_t t) { return {t}; }
  static std::uint32_t toThrift(std::bitset<32> t) { return t.to_ullong(); }
};
```
```
// In thrift file
include "thrift/annotation/cpp.thrift"
cpp_include "BitsetAdapter.h"
struct Foo {
  @cpp.Adapter{name = "BitsetAdapter"}
  1: i32 flags;
}
```
```cpp
// In C++ file
Foo foo;
foo.flags()->set(0); // set 0th bit
```
> **Note: cpp.Adapter will break Thrift py3 usage. Please migrate to new Thrift Python to use cpp.Adapter and python.Adapter.**

> **Note: Adapted types can not be used directly with Thrift APIs. Values must be converted back to Thrift types. (e.g. `apache::thrift::CompactSerializer::serialize(Adapter::toThrift(adaptedValue))`)**

### Type Adapter

Type adapter’s APIs are mentioned above. It’s worth noting that API doesn’t need to be identical — it can be template function. In addition, the user can use reference to avoid making extra copy/move. e.g.

```cpp
struct ThriftTypeAdapter {
  static AdaptedType fromThrift(DefaultType&& thrift);
  static const DefaultType& toThrift(const AdaptedType& native);
};
```
### Field Adapter

Field adapter requires the user to provide the following APIs.

```cpp
struct ThriftFieldAdapter {
  template<class Context>
  static void construct(AdaptedType& thrift, Context ctx);

  template<class Context>
  static AdaptedType fromThriftField(DefaultType thrift, Context ctx);

  static {const DefaultType& | DefaultType} toThrift(
      const AdaptedType& native);
};
```
`Context` is an instantiation of [FieldAdapterContext](https://www.internalfb.com/code/fbsource/[388310ab2f4d793c6b1056369cedfa9fd12ff51b]/fbcode/thrift/lib/cpp2/Adapt.h?lines=31) which can access field id and parent struct. e.g.

```cpp
template<class Context>
AdaptedType ThriftFieldAdapter::fromThriftField(DefaultType thrift, Context ctx) {
  constexpr int16_t kFieldId = Context::kFieldId;
  auto& object s = ctx.object;
  // ...
}
```
Both type adapter and field adapter use `cpp.Adapter` API to enable. We will try to invoke field adapter API first. If it fails, we will fall back to type adapter.

### Compose

You cannot apply multiple field adapters, nor type adapters on the same type, but you can use both field adapter and type adapter together,. e.g.

```
@cpp.Adapter{name = "CustomTypeAdapter"}
typedef i32 CustomInt

struct Foo {
  @cpp.Adapter{name = "CustomFieldAdapter"}
  1: CustomInt field;

  @cpp.Adapter{name = "CustomTypeAdapter"}
  2: CustomInt field2;
}
```
More examples can be found [here](https://www.internalfb.com/code/fbsource/fbcode/thrift/test/adapter.thrift).

### Customization for Optimization

Thrift Adapter allows further customization points to avoid calling `fromThrift` and `toThrift` for optimization. Assume `obj` has type `AdaptedType`.

* Comparison and Equality

   * Comparison Priority
      * `Adapter::equal(const AdaptedType& lhs, const AdaptedType& rhs)`
      * `AdaptedType::operator==(const AdaptedType& rhs)`
      * `DefaultType::operator==(const DefaultType& rhs)` using `Adapter::toThrift(lhs) == Adapter::toThrift(rhs)`
   * Equality Priority is same as above.
* Hash

   * Hash Priority
      * `std::hash<AdaptedType>(obj)`
      * `std::hash<DefaultType>(Adapter::toThrift(obj))`
* Clear

   * Clear Priority
      * `Adapter::clear(obj)`
      * `obj = AdaptedType()`
* Serialized Size

   * SerializeSize Priority
      * `Adapter::serializedSize(Protocol&, AdaptedType&)`
      * `Protocol::serializedSize(Adapter::toThrift(obj))`
* Serialize and Deserialize

   * TBD

### Other Codegen Customizations

 * `adaptedType`: normally the runtime can determine the result of `Adapter::fromThrift`, but sometimes doing so would result in a circular dependency or declaration order error. Setting this annotation to the result type breaks the dependency.
 * `underlyingName` and `extraNamespace`: when directly adapting types, the underlying type needs to be mangled to avoid colliding with the adapted type name. If neither is specified thrift will use the `detail` namespace and the same name.
 * `moveOnly`: indicates that structs with this adapted type as a field should not have copy constructors.

## Hack

### Type Adapter

#### On Typedef

Hack type adapter can be enabled via [`hack.Adapter`](https://www.internalfb.com/code/fbsource/[8155292d2d170907326ad48d3a070ebe3ccb1be3]/fbcode/thrift/annotation/hack.thrift?lines=55). It should implement [`IThriftAdapter` ](https://www.internalfb.com/code/www/[1cce1dcd39b3d25482944ebc796fafb4c3a3d4fd]/flib/thrift/core/IThriftAdapter.php?lines=19-29)interface. For example

```hack
// In Hack file
final class TimestampToTimeAdapter implements IThriftAdapter {
  const type TThriftType = int;
  const type THackType = Time;
  public static function fromThrift(int $seconds)[]: Time {
    return Time::fromEpochSeconds($seconds);
  }
  public static function toThrift(Time $time): int {
    return $hack_value->asFullSecondsSinceEpoch();
  }
}
```
```
// In thrift file
include "thrift/annotation/hack.thrift"
@hack.Adapter {name = '\TimestampToTimeAdapter'}
typedef i32 i32_withAdapter;

struct Document {
  1: i32_withAdapter created_time;
}
```
```
// Thrift compiler will generate this for you:
class Document implements \IThriftSyncStruct {
...
  public TimestampToTimeAdapter::THackType $created_time;
...
}
```
```
// In hack file
function timeSinceCreated(Document $doc): Duration {
  // $doc->created_time is of type Time
  return Duration::between(Time::now(), $doc->created_time);
}
```
#### On Field

Similar to typedef, adapter can be added to fields directly. It should implement [`IThriftAdapter` ](https://www.internalfb.com/code/www/[1cce1dcd39b3d25482944ebc796fafb4c3a3d4fd]/flib/thrift/core/IThriftAdapter.php?lines=19-29)interface. Below IDL will generate the same code as above.

```
// In thrift file
struct Document {
  @hack.Adapter{name = '\TimestampToTimeAdapter'}
  1: i32 created_time;
}
// Thrift compiler will generate this for you:
class Document implements \IThriftSyncStruct {
...
  public TimestampToTimeAdapter::THackType $created_time;
...
}
```
### Field Wrapper

This annotation will let you change the field’s type to a wrapper class. It should extend  [`IThriftFieldWrapper`](https://www.internalfb.com/code/www/flib/thrift/core/IThriftFieldWrapper.php?lines=22)  class. FieldWrapper wraps your field within the wrapper class and allows you to execute custom code before reading/writing the field. As a result, the field becomes private, and you can get the wrapper object via the getter method in the struct. The wrapper APIs are async so using wrapped fields will change your struct to an async struct. All the factory methods like `fromShape`, `toShape` will become async to access field value via the wrapper.

See example below.

```
// In Hack file
final class FooOnlyAllowOwnerToAccess<TThriftType, TStructType>
    extends IThriftFieldWrapper<TThriftType, TStructType> {

  <<__Override>>
  public static async function genToThrift(
    this $value,
  )[zoned]: Awaitable<TThriftType> {
    return await $value->genUnwrap();
  }

  <<__Override>>
  public static async function genFromThrift<
    <<__Explicit>> TThriftType__,
    <<__Explicit>> TThriftStruct__ as IThriftAsyncStruct,
  >(
    TThriftType__ $value,
    int $field_id,
    TThriftStruct__ $parent,
  )[zoned]: Awaitable<MyFieldWrapper<TThriftType__, TThriftStruct__>> {
    return new MyFieldWrapper($value, $field_id, $parent);
  }

  <<__Override>>
  public async function genUnwrap()[zoned]: Awaitable<TThriftType> {
    await genHasAccess();
    return $this->value;
  }

  <<__Override>>
  public async function genWrap(TThriftType $value)[zoned]: Awaitable<void> {
    await genHasAccess();
    $this->value = $value;
  }

  public async function genHasAccess(): Awaitable<void> {
       ...
    if ($has_access) {
        return;
    }
    throw new InvalidAccessException();
  }

  public async function genAsTime()[zoned]:  Awaitable<Time> {
      await genHasAccess();
      return Time::fromEpochSeconds($this->value as int);
  }

}
```
```
// In thrift file
struct Document {
  @hack.FieldWrapper{name = '\FooOnlyAllowOwnerToAccess'}
  1: i32 created_time;
}
```
```
// Thrift compiler will generate this for you:
class Document implements \IThriftAsyncStruct {
...
  private ?\FooOnlyAllowOwnerToAccess<int, Document> $created_time;
  public function get_created_time()[]: \FooOnlyAllowOwnerToAccess<int, Document> {
    $this->created_time as nonnull;
  }

  public async genFromShape(TConstructorShape $shape)[zoned_local]: Awaitable<this> {
    ...
  }
...
}
```
```
// In hack file
function timeSinceCreated(Document $doc): Duration {
  $time = await $doc->get_created_time()->genAsTime();
  return Duration::between(Time::now(), $time);
}
```
### Compose

* You can only apply one adapter to a field either via field adapter or via type adapter.
* You can only apply one field wrapper to the field.
* A field wrapper and type/field adapter can be used together, e.g.

```
@hack.Adapter{name = "CustomTypeAdapter"}
typedef i32 CustomInt

struct Foo {
  @hack.FieldWrapper{name = "CustomFieldWrapper"}
  1: CustomInt field1;
  @hack.Adapter{name = "CustomFieldAdapter"}
  @hack.FieldWrapper{name = "CustomFieldWrapper"}
  2: i32 field2;
}
```
## Python

Python adapter can be enabled via `python.Adapter`. e.g.

```python
# In Python file
class DatetimeAdapter(Adapter[int, datetime]):
    @classmethod
    def from_thrift(cls, original: int) -> datetime:
        return datetime.fromtimestamp(original)

    @classmethod
    def to_thrift(cls, adapted: datetime) -> int:
        return int(adapted.timestamp())
```
```
// In thrift file
include "thrift/annotation/python.thrift"
struct Foo {
  @python.Adapter{
    name = "DatetimeAdapter",
    typeHint = "datetime.datetime",
  }
  1: i32 dt;
}
```
```python
// In python file
foo = Foo()
assert isinstance(foo.dt, datetime)
assert foo.dt.timestamp() == 0
```
Please check thrift-python wiki page for more detail: https://www.internalfb.com/intern/wiki/Thrift-Python/User_Guide/Advanced_Usage/Adapter/.

## Java

### Type Adapter

Java type adapters can be enabled via `java.Adapter` annotation by providing the `adapterClassName` and `typeClassName` parameters. Type adapters are only supported when using java2. Adapters can be applied to typedef or fields.

```
// include java.thrift for java.Adapter annotation
include "thrift/annotation/java.thrift"

@java.Adapter{
  adapterClassName = "com.facebook.thrift.adapter.common.DateTypeAdapter",
  typeClassName = "java.util.Date",
}
typedef i64 Date

struct MyStruct {
  1: Date date;
}
```

It can also be applied to a field.

```
// include java.thrift for java.Adapter annotation
include "thrift/annotation/java.thrift"

struct MyStruct {
  @java.Adapter{
    adapterClassName = "com.facebook.thrift.adapter.common.DateTypeAdapter",
    typeClassName = "java.util.Date",
  }
  1: i64 date;
}
```

Adapters defined on typedef level can be used on nested container types.

```
// include java.thrift for java.Adapter annotation
include "thrift/annotation/java.thrift"

@java.Adapter{
  adapterClassName = "com.facebook.thrift.adapter.common.DateTypeAdapter",
  typeClassName = "java.util.Date",
}
typedef i64 Date

struct MyStruct {
  1: list<Date> date_list;
}
```

#### Defining new Type Adapter

A new type adapter can be defined by implementing the [TypeAdapter](https://github.com/facebook/fbthrift/blob/main/thrift/lib/java/common/src/main/java/com/facebook/thrift/adapter/TypeAdapter.java) interface.

```java
public interface TypeAdapter<T, P> extends Adapter<P> {
  /**
   * Converts given type to the adapted type.
   *
   * @param t Thrift type or an adapted type.
   * @return Adapted type
   */
  P fromThrift(T t);

  /**
   * Converts adapted type to the original type.
   *
   * @param p Adapted type
   * @return Thrift type or an adapted type
   */
  T toThrift(P p);
}
```

Thrift Java already defines a type adapter interface for each thrift type by extending the TypeAdapter interface. It is recommended to implement one of the existing interfaces defined in [com.facebook.thrift.adapter.common](https://github.com/facebook/fbthrift/tree/main/thrift/lib/java/common/src/main/java/com/facebook/thrift/adapter/common) package when creating a new type adapter. There are also predefined type adapters in the same package for `java.util.Date`, `CopiedPooledByteBuf`, `RetainedSlicedPooledByteBuf` and `UnpooledByteBuf`.


```java
public interface IntegerTypeAdapter<T> extends TypeAdapter<Integer, T> {}

public class DateTypeAdapter implements LongTypeAdapter<Date> {
  @Override
  public Date fromThrift(Long aLong) {
    return new Date(aLong);
  }

  @Override
  public Long toThrift(Date date) {
    return date == null ? null : date.getTime();
  }
}

/**
 * Use this type adapter if you want a to get a zero copy slice of the binary field. This will
 * create a retained slice so you must free the ByteBuf when you're not longer interested in using
 * it to prevent memory leaks. Use with caution.
 */
public class RetainedSlicedPooledByteBufTypeAdapter implements TypeAdapter<ByteBuf, ByteBuf> {
  @Override
  public ByteBuf fromThrift(ByteBuf byteBuf) {
    return byteBuf.retain();
  }

  /** Input byteBuf is released when it is written to the IO stream. */
  @Override
  public ByteBuf toThrift(ByteBuf byteBuf) {
    return byteBuf;
  }
}
```

Additionally you might need to refer to packages that aren’t in the standard classpath. You can do this in your `TARGETS` file by adding `java_deps` to your `thrift_library` buck target. Here’s an example that adds netty-buffer to the classpath:

```
thrift_library(
  name = "type-adapter",
  java_deps = [
    "//third-party-java/io.netty:netty-buffer",
  ],
  languages = [
    "java2",
  ],
  thrift_srcs = {"type_adapter.thrift": []},
  deps = [
      "//thrift/annotation:java",
  ],
)
```

### Wrapper

Field adapters are defined as wrappers in Java. Wrappers can be enabled via `java.Wrapper` annotation by providing the `wrapperClassName` and `typeClassName` parameters. Wrappers are only supported when using java2. It can only be applied to a field. Wrappers provide access to the underlying struct/exception and allows executing custom code before accessing the field value.

```
// include java.thrift for java.Wrapper annotation
include "thrift/annotation/java.thrift"

struct MyStruct {
  @java.Wrapper{
    wrapperClassName = "com.facebook.thrift.wrapper.test.FieldWrapper<Integer>",
    typeClassName = "com.facebook.thrift.wrapper.test.PoliciedField<Integer>",
  }
  1: i32 field1;
}
```

#### Defining new Wrapper

A new wrapper can be defined by implementing the [Wrapper](https://github.com/facebook/fbthrift/blob/main/thrift/lib/java/common/src/main/java/com/facebook/thrift/adapter/Wrapper.java) interface.

```java
public interface Wrapper<T, P, R> extends Adapter<P> {
  /**
   * Converts given type to the wrapped type.
   *
   * @param t Thrift type or an adapted type.
   * @param fieldContext field context including field id and root object
   * @return Wrapped type
   */
  P fromThrift(T t, FieldContext<R> fieldContext);

  /**
   * Converts wrapped type to the original type.
   *
   * @param p Wrapped type
   * @return Thrift type or an adapted type
   */
  T toThrift(P p);
}
```

[FieldContext](https://github.com/facebook/fbthrift/blob/main/thrift/lib/java/common/src/main/java/com/facebook/thrift/adapter/FieldContext.java) object provides access to the field id of the struct/exception which the wrapper is applied and the parent object (struct/exception).

```java
public class FieldContext<T> {
  private int fieldId;
  private T t;

  public FieldContext(int fieldId, T t) {
    this.fieldId = fieldId;
    this.t = t;
  }

  public int getFieldId() {
    return fieldId;
  }

  // this will return the root object
  public T getRootObj() {
    return t;
  }
}
```

### Compose
You cannot apply multiple wrappers, nor type adapters on the same type, but you can use both wrapper and type adapter together. e.g.

```
@java.Adapter{
  adapterClassName = "com.facebook.thrift.CustomTypeAdapter",
  typeClassName = "com.facebook.CustomInt",
}
typedef i32 CustomInt

struct Foo {
  @java.Wrapper{
    wrapperClassName = "com.facebook.thrift.CustomWrapper",
    typeClassName = "com.facebook.thrift.PoliciedField<com.facebook.CustomInt>",
  }
  1: CustomInt field1;

  @java.Adapter{
    adapterClassName = "com.facebook.thrift.CustomDateAdapter",
    typeClassName = "com.facebook.test.CustomDate",
  }
  2: CustomInt field2;
}
```
