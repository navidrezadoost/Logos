import React from "react";
import type { SVGProps } from "react";

export interface BoolIntersectProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const BoolIntersect = React.forwardRef<SVGSVGElement, BoolIntersectProps>(
  ({ size, className, style, ...props }, ref) => {
    const dimensions = size ? { width: size, height: size } : {
      width: props.width || 24,
      height: props.height || 24
    };

    return (
      <svg
        ref={ref}
        viewBox="0 0 640 640"
        xmlns="http://www.w3.org/2000/svg"
        width={dimensions.width}
        height={dimensions.height}
        fill={props.fill || "currentColor"}
        className={className}
        style={style}
        {...props}
      >
        <path fill="currentColor" d="M352 128L352 224L224 224L224 352L128 352L128 128L352 128zM128 416L224 416L224 576L576 576L576 224L416 224L416 64L64 64L64 416L128 416zM512 288L512 512L288 512L288 416L416 416L416 288L512 288z"/>
      </svg>
    );
  }
);

BoolIntersect.displayName = "BoolIntersect";

export default BoolIntersect;
