import React, { useState, useEffect } from 'react';

export function Counter() {
  const [count, setCount] = useState(0);
  useEffect(() => {
    console.log(count);
  }, [count]);
  return count;
}
