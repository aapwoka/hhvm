<?hh

interface I {
  public function foo(): void;
}

class A implements I {
  public function foo(): void {}
}

class B extends A {
  public function bar(int $_): void {}
}